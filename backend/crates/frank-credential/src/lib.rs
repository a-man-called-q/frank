use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential store input is invalid: {0}")]
    InvalidAddress(String),
    #[error("native credential store unavailable: {0}")]
    Tls(String),
    #[error("native credential readback did not match the value written")]
    ReadbackMismatch,
    #[error("safe credential IO failed: {0}")]
    SafeIo(#[from] frank_safeio::SafeIoError),
}

pub type Result<T> = std::result::Result<T, CredentialError>;

/// The storage boundary for daemon and desktop bearer credentials.
pub trait CredentialStore: Send + Sync {
    fn save(&self, reference: &str, token: &str) -> Result<()>;
    fn load(&self, reference: &str) -> Result<Option<String>>;
    fn delete(&self, reference: &str) -> Result<()>;
    fn warning(&self) -> Option<&'static str>;

    /// Returns a non-secret label for the backend that supplied a token.
    fn source(&self, reference: &str) -> Option<&'static str> {
        let _ = reference;
        None
    }
}

#[derive(Debug, Clone)]
pub struct FileCredentialStore {
    pub root: PathBuf,
}

impl FileCredentialStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, reference: &str) -> PathBuf {
        self.root
            .join(format!("{}.token", sanitize_reference(reference)))
    }
}

impl CredentialStore for FileCredentialStore {
    fn save(&self, reference: &str, token: &str) -> Result<()> {
        frank_safeio::ensure_dir(&self.root).map_err(CredentialError::SafeIo)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| CredentialError::InvalidAddress(error.to_string()))?;
        }
        let path = self.path(reference);
        if std::fs::symlink_metadata(&path).is_ok() {
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| CredentialError::InvalidAddress(error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(CredentialError::InvalidAddress(
                    "credential path is a symlink".into(),
                ));
            }
        }
        frank_safeio::write_text_atomic(&path, token, 64 * 1024)
            .map_err(CredentialError::SafeIo)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| CredentialError::InvalidAddress(error.to_string()))?;
        }
        Ok(())
    }

    fn load(&self, reference: &str) -> Result<Option<String>> {
        let path = self.path(reference);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return Ok(None);
        };
        if metadata.file_type().is_symlink() {
            return Err(CredentialError::InvalidAddress(
                "credential path is a symlink".into(),
            ));
        }
        frank_safeio::read_text_capped(&path, 64 * 1024)
            .map(Some)
            .map_err(CredentialError::SafeIo)
    }

    fn delete(&self, reference: &str) -> Result<()> {
        let path = self.path(reference);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(
                CredentialError::InvalidAddress("credential path is a symlink".into()),
            ),
            Ok(_) => std::fs::remove_file(path)
                .map_err(|error| CredentialError::InvalidAddress(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(CredentialError::InvalidAddress(error.to_string())),
        }
    }

    fn warning(&self) -> Option<&'static str> {
        Some(
            "OS Secret Service/keychain unavailable; using a symlink-safe mode-0600 token fallback",
        )
    }

    fn source(&self, reference: &str) -> Option<&'static str> {
        self.load(reference).ok().flatten().map(|_| "file")
    }
}

/// Namespaces used by the two native clients.  The daemon intentionally does
/// not share a keychain namespace with the desktop application.
pub const DESKTOP_SERVICE: &str = "dev.frank.desktop";
pub const DAEMON_SERVICE: &str = "dev.frank.daemon";
pub const DESKTOP_LABEL: &str = "Frank desktop device token";
pub const DAEMON_LABEL: &str = "Frank daemon credential";
pub const MAX_CREDENTIAL_BYTES: usize = 64 * 1024;

/// Platform-independent seam around a native credential manager.  Keeping
/// this trait public lets callers test migration and fallback behavior without
/// launching `security`, `secret-tool`, or Win32 APIs.
pub trait NativeCredentialBackend: Send + Sync {
    fn save(&self, service: &str, label: &str, reference: &str, token: &str) -> Result<()>;
    fn load(&self, service: &str, reference: &str) -> Result<Option<String>>;
    fn delete(&self, service: &str, reference: &str) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
struct PlatformNativeCredentialBackend;

impl NativeCredentialBackend for PlatformNativeCredentialBackend {
    #[allow(clippy::needless_return)]
    fn save(&self, service: &str, label: &str, reference: &str, token: &str) -> Result<()> {
        let account = sanitize_reference(reference);
        let key = format!("{service}:{account}");
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("security")
                .args([
                    "add-generic-password",
                    "-a",
                    "frank",
                    "-s",
                    &key,
                    "-l",
                    label,
                    "-w",
                    token,
                    "-U",
                ])
                .output();
            return match output {
                Ok(output) if output.status.success() => Ok(()),
                Ok(_) | Err(_) => Err(CredentialError::Tls("macOS Keychain unavailable".into())),
            };
        }
        #[cfg(target_os = "linux")]
        {
            let mut command = std::process::Command::new("secret-tool");
            command.args([
                "store",
                &format!("--label={label}"),
                "service",
                service,
                "account",
                &account,
            ]);
            command.stdin(std::process::Stdio::piped());
            return match command.spawn() {
                Ok(mut child) => {
                    let result = child
                        .stdin
                        .take()
                        .map(|mut stdin| std::io::Write::write_all(&mut stdin, token.as_bytes()));
                    let status = child.wait();
                    if matches!(result, Some(Ok(()))) && status.is_ok_and(|status| status.success())
                    {
                        Ok(())
                    } else {
                        Err(CredentialError::Tls(
                            "Linux Secret Service unavailable".into(),
                        ))
                    }
                }
                Err(_) => Err(CredentialError::Tls(
                    "Linux Secret Service unavailable".into(),
                )),
            };
        }
        #[cfg(target_os = "windows")]
        {
            let _ = (service, label);
            return windows_credentials::save(&key, token);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = (service, label, reference, token);
            Err(CredentialError::Tls(
                "native credential store unavailable".into(),
            ))
        }
    }

    #[allow(clippy::needless_return)]
    fn load(&self, service: &str, reference: &str) -> Result<Option<String>> {
        let account = sanitize_reference(reference);
        let key = format!("{service}:{account}");
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("security")
                .args(["find-generic-password", "-a", "frank", "-s", &key, "-w"])
                .output();
            return match output {
                Ok(output) if output.status.success() => Ok(Some(
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                )),
                Ok(output) if output.status.code() == Some(44) => Ok(None),
                Ok(_) | Err(_) => Err(CredentialError::Tls("macOS Keychain unavailable".into())),
            };
        }
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("secret-tool")
                .args(["lookup", "service", service, "account", &account])
                .output();
            return match output {
                Ok(output) if output.status.success() => Ok(Some(
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                )),
                Ok(output) if output.status.code() == Some(1) => Ok(None),
                Ok(_) | Err(_) => Err(CredentialError::Tls(
                    "Linux Secret Service unavailable".into(),
                )),
            };
        }
        #[cfg(target_os = "windows")]
        {
            let _ = service;
            return windows_credentials::load(&key);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = (service, reference);
            Err(CredentialError::Tls(
                "native credential store unavailable".into(),
            ))
        }
    }

    #[allow(clippy::needless_return)]
    fn delete(&self, service: &str, reference: &str) -> Result<()> {
        let account = sanitize_reference(reference);
        let key = format!("{service}:{account}");
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("security")
                .args(["delete-generic-password", "-a", "frank", "-s", &key])
                .output();
            return match output {
                Ok(output) if output.status.success() || output.status.code() == Some(44) => Ok(()),
                Ok(_) | Err(_) => Err(CredentialError::Tls("macOS Keychain unavailable".into())),
            };
        }
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("secret-tool")
                .args(["clear", "service", service, "account", &account])
                .output();
            return match output {
                Ok(output) if output.status.success() || output.status.code() == Some(1) => Ok(()),
                Ok(_) | Err(_) => Err(CredentialError::Tls(
                    "Linux Secret Service unavailable".into(),
                )),
            };
        }
        #[cfg(target_os = "windows")]
        {
            let _ = service;
            return windows_credentials::delete(&key);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = (service, reference);
            Err(CredentialError::Tls(
                "native credential store unavailable".into(),
            ))
        }
    }
}

/// Native credential-store adapter.  A daemon store reads the new daemon
/// namespace first, then one-way migrates an existing desktop entry only
/// after a successful write/readback.  A failed migration leaves the legacy
/// entry untouched and still returns it for compatibility.
#[derive(Clone)]
pub struct NativeCredentialStore {
    pub service: String,
    pub label: String,
    pub fallback: FileCredentialStore,
    backend: Arc<dyn NativeCredentialBackend>,
}

impl std::fmt::Debug for NativeCredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCredentialStore")
            .field("service", &self.service)
            .field("label", &self.label)
            .field("fallback", &self.fallback)
            .finish_non_exhaustive()
    }
}

impl NativeCredentialStore {
    /// Construct a store with an explicit native service, label, and file
    /// fallback root.  Prefer [`Self::desktop`] or [`Self::daemon`] at call
    /// sites so namespaces remain visible in the composition root.
    pub fn new(
        service: impl Into<String>,
        label: impl Into<String>,
        root: impl Into<PathBuf>,
    ) -> Self {
        Self::with_backend(service, label, root, PlatformNativeCredentialBackend)
    }

    pub fn desktop(root: impl Into<PathBuf>) -> Self {
        Self::new(DESKTOP_SERVICE, DESKTOP_LABEL, root)
    }

    pub fn daemon(root: impl Into<PathBuf>) -> Self {
        Self::new(DAEMON_SERVICE, DAEMON_LABEL, root)
    }

    pub fn with_backend(
        service: impl Into<String>,
        label: impl Into<String>,
        root: impl Into<PathBuf>,
        backend: impl NativeCredentialBackend + 'static,
    ) -> Self {
        Self {
            service: service.into(),
            label: label.into(),
            fallback: FileCredentialStore::new(root),
            backend: Arc::new(backend),
        }
    }

    fn native_load(&self, service: &str, reference: &str) -> Result<Option<String>> {
        self.backend.load(service, reference)
    }

    fn migrate_legacy(&self, reference: &str, token: &str) {
        if self.service == DESKTOP_SERVICE {
            return;
        }
        let Ok(()) = self
            .backend
            .save(&self.service, &self.label, reference, token)
        else {
            return;
        };
        if matches!(
            self.native_load(&self.service, reference),
            Ok(Some(ref value)) if value == token
        ) {
            // Delete only after readback proves the new namespace is usable.
            // Failure to remove the old entry is harmless and preserves the
            // user's recoverable credential.
            let _ = self.backend.delete(DESKTOP_SERVICE, reference);
        }
    }
}

impl CredentialStore for NativeCredentialStore {
    fn save(&self, reference: &str, token: &str) -> Result<()> {
        if token.len() > MAX_CREDENTIAL_BYTES {
            return Err(CredentialError::InvalidAddress(format!(
                "credential exceeds the {MAX_CREDENTIAL_BYTES}-byte safety cap"
            )));
        }
        match self
            .backend
            .save(&self.service, &self.label, reference, token)
        {
            Ok(()) => {
                if matches!(
                    self.native_load(&self.service, reference),
                    Ok(Some(ref value)) if value == token
                ) {
                    return Ok(());
                }
                // Do not leave a partially written native value shadowing the
                // compatible file fallback after a failed readback.
                let _ = self.backend.delete(&self.service, reference);
                self.fallback.save(reference, token)
            }
            Err(_) => self.fallback.save(reference, token),
        }
    }

    fn load(&self, reference: &str) -> Result<Option<String>> {
        match self.native_load(&self.service, reference) {
            Ok(Some(value)) => return Ok(Some(value)),
            Ok(None) if self.service != DESKTOP_SERVICE => {
                if let Ok(Some(value)) = self.native_load(DESKTOP_SERVICE, reference) {
                    self.migrate_legacy(reference, &value);
                    return Ok(Some(value));
                }
            }
            Ok(None) | Err(_) => {}
        }
        self.fallback.load(reference)
    }

    fn delete(&self, reference: &str) -> Result<()> {
        let native = self.backend.delete(&self.service, reference);
        if self.service != DESKTOP_SERVICE {
            let _ = self.backend.delete(DESKTOP_SERVICE, reference);
        }
        match native {
            Ok(()) => self.fallback.delete(reference),
            Err(_) => self.fallback.delete(reference),
        }
    }

    fn warning(&self) -> Option<&'static str> {
        self.fallback.warning()
    }

    fn source(&self, reference: &str) -> Option<&'static str> {
        let native_present = self
            .native_load(&self.service, reference)
            .ok()
            .flatten()
            .is_some()
            || (self.service != DESKTOP_SERVICE
                && self
                    .native_load(DESKTOP_SERVICE, reference)
                    .ok()
                    .flatten()
                    .is_some());
        if native_present {
            Some("keychain")
        } else if self.fallback.load(reference).ok().flatten().is_some() {
            Some("file")
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
mod windows_credentials {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::slice;

    use super::Result;

    const CRED_TYPE_GENERIC: u32 = 1;
    const CRED_PERSIST_LOCAL: u32 = 2;
    const ERROR_NOT_FOUND: u32 = 1168;

    #[repr(C)]
    struct FileTime {
        dwLowDateTime: u32,
        dwHighDateTime: u32,
    }

    #[repr(C)]
    struct CredentialAttributeW {
        Keyword: *mut u16,
        Flags: u32,
        ValueSize: u32,
        Value: *mut u8,
    }

    #[repr(C)]
    struct CredentialW {
        Flags: u32,
        Type: u32,
        TargetName: *mut u16,
        Comment: *mut u16,
        LastWritten: FileTime,
        CredentialBlobSize: u32,
        CredentialBlob: *mut u8,
        Persist: u32,
        AttributeCount: u32,
        Attributes: *mut CredentialAttributeW,
        TargetAlias: *mut u16,
        UserName: *mut u16,
    }

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        fn CredReadW(
            target_name: *const u16,
            credential_type: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        fn CredDeleteW(target_name: *const u16, credential_type: u32, flags: u32) -> i32;
        fn CredFree(buffer: *mut c_void);
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn save(target: &str, token: &str) -> Result<()> {
        let target = wide(target);
        let mut blob = token.as_bytes().to_vec();
        let credential = CredentialW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_ptr().cast_mut(),
            Comment: ptr::null_mut(),
            LastWritten: FileTime {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: ptr::null_mut(),
        };
        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok == 0 {
            return Err(CredentialError::Tls(format!(
                "Windows Credential Manager write failed ({})",
                unsafe { GetLastError() }
            )));
        }
        Ok(())
    }

    pub fn load(target: &str) -> Result<Option<String>> {
        let target = wide(target);
        let mut credential = ptr::null_mut();
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            return if error == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(CredentialError::Tls(format!(
                    "Windows Credential Manager read failed ({error})"
                )))
            };
        }
        if credential.is_null() {
            return Err(CredentialError::Tls(
                "Windows Credential Manager returned an empty credential".into(),
            ));
        }
        let result = unsafe {
            let value = slice::from_raw_parts(
                (*credential).CredentialBlob,
                (*credential).CredentialBlobSize as usize,
            );
            String::from_utf8(value.to_vec())
                .map_err(|_| CredentialError::Tls("Windows credential is not UTF-8".into()))
        };
        unsafe { CredFree(credential.cast()) };
        result.map(Some)
    }

    pub fn delete(target: &str) -> Result<()> {
        let target = wide(target);
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok != 0 || unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(CredentialError::Tls(
                "Windows Credential Manager delete failed".into(),
            ))
        }
    }
}

fn sanitize_reference(reference: &str) -> String {
    let mut value = reference
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect::<String>();
    if value.is_empty() {
        value = "default".into();
    }
    value.chars().take(96).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Default)]
    struct FakeState {
        entries: HashMap<(String, String), String>,
        fail_all: bool,
        fail_daemon_save: bool,
        mismatch_daemon_readback: bool,
    }

    #[derive(Clone, Debug, Default)]
    struct FakeNativeBackend(Arc<Mutex<FakeState>>);

    impl FakeNativeBackend {
        fn with_state(state: FakeState) -> Self {
            Self(Arc::new(Mutex::new(state)))
        }

        fn token(&self, service: &str, reference: &str) -> Option<String> {
            self.0
                .lock()
                .expect("fake backend lock")
                .entries
                .get(&(service.into(), reference.into()))
                .cloned()
        }
    }

    impl NativeCredentialBackend for FakeNativeBackend {
        fn save(&self, service: &str, _label: &str, reference: &str, token: &str) -> Result<()> {
            let mut state = self.0.lock().expect("fake backend lock");
            if state.fail_all || (service == DAEMON_SERVICE && state.fail_daemon_save) {
                return Err(CredentialError::Tls("fake native write failure".into()));
            }
            let value = if service == DAEMON_SERVICE && state.mismatch_daemon_readback {
                "different-token"
            } else {
                token
            };
            state
                .entries
                .insert((service.into(), reference.into()), value.into());
            Ok(())
        }

        fn load(&self, service: &str, reference: &str) -> Result<Option<String>> {
            let state = self.0.lock().expect("fake backend lock");
            if state.fail_all {
                return Err(CredentialError::Tls("fake native read failure".into()));
            }
            Ok(state
                .entries
                .get(&(service.into(), reference.into()))
                .cloned())
        }

        fn delete(&self, service: &str, reference: &str) -> Result<()> {
            let mut state = self.0.lock().expect("fake backend lock");
            if state.fail_all {
                return Err(CredentialError::Tls("fake native delete failure".into()));
            }
            state.entries.remove(&(service.into(), reference.into()));
            Ok(())
        }
    }

    #[test]
    fn daemon_migrates_legacy_namespace_only_after_readback() {
        let backend = FakeNativeBackend::with_state(FakeState {
            entries: HashMap::from([((DESKTOP_SERVICE.into(), "owner".into()), "secret".into())]),
            ..FakeState::default()
        });
        let store = NativeCredentialStore::with_backend(
            DAEMON_SERVICE,
            DAEMON_LABEL,
            tempfile::tempdir().unwrap().path(),
            backend.clone(),
        );

        assert_eq!(store.load("owner").unwrap().as_deref(), Some("secret"));
        assert_eq!(
            backend.token(DAEMON_SERVICE, "owner").as_deref(),
            Some("secret")
        );
        assert_eq!(backend.token(DESKTOP_SERVICE, "owner"), None);
    }

    #[test]
    fn failed_migration_preserves_legacy_entry() {
        let backend = FakeNativeBackend::with_state(FakeState {
            entries: HashMap::from([((DESKTOP_SERVICE.into(), "owner".into()), "secret".into())]),
            fail_daemon_save: true,
            ..FakeState::default()
        });
        let store = NativeCredentialStore::with_backend(
            DAEMON_SERVICE,
            DAEMON_LABEL,
            tempfile::tempdir().unwrap().path(),
            backend.clone(),
        );

        assert_eq!(store.load("owner").unwrap().as_deref(), Some("secret"));
        assert_eq!(
            backend.token(DESKTOP_SERVICE, "owner").as_deref(),
            Some("secret")
        );
        assert_eq!(backend.token(DAEMON_SERVICE, "owner"), None);
    }

    #[test]
    fn readback_failure_uses_file_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeNativeBackend::with_state(FakeState {
            mismatch_daemon_readback: true,
            ..FakeState::default()
        });
        let store = NativeCredentialStore::with_backend(
            DAEMON_SERVICE,
            DAEMON_LABEL,
            directory.path(),
            backend,
        );

        store.save("owner", "secret").unwrap();
        assert_eq!(store.load("owner").unwrap().as_deref(), Some("secret"));
        assert_eq!(store.source("owner"), Some("file"));
    }

    #[test]
    fn unavailable_native_store_keeps_file_compatibility() {
        let directory = tempfile::tempdir().unwrap();
        let backend = FakeNativeBackend::with_state(FakeState {
            fail_all: true,
            ..FakeState::default()
        });
        let store = NativeCredentialStore::with_backend(
            DAEMON_SERVICE,
            DAEMON_LABEL,
            directory.path(),
            backend,
        );

        store.save("owner", "secret").unwrap();
        assert_eq!(store.load("owner").unwrap().as_deref(), Some("secret"));
        assert_eq!(store.source("owner"), Some("file"));
    }

    #[cfg(unix)]
    #[test]
    fn file_fallback_is_private_bounded_and_symlink_safe() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::new(directory.path());
        store.save("owner", "secret").unwrap();
        let path = directory.path().join("owner.token");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(directory.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert!(matches!(
            store.save("large", &"x".repeat(MAX_CREDENTIAL_BYTES + 1)),
            Err(CredentialError::SafeIo(
                frank_safeio::SafeIoError::TooLarge(_)
            ))
        ));

        let target = directory.path().join("outside");
        std::fs::write(&target, "outside").unwrap();
        let link = directory.path().join("linked.token");
        symlink(&target, &link).unwrap();
        assert!(store.load("linked").is_err());
        assert!(store.save("linked", "nope").is_err());
        assert!(store.delete("linked").is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "outside");
    }
}
