//! Local third-party pack storage.
//!
//! A pack is data, not an executable plugin: `pack add` validates the TOML,
//! compiles every prompt and regex, copies only regular files, and records a
//! deterministic directory digest in `packs.lock`. Selecting a pack compiles
//! the locked copy again and refuses to run if it changed underneath the
//! lockfile.
//!
//! Remote sources are intentionally not implemented here. The approved M7
//! design allows GitHub/HTTPS sources, but adding a downloader also adds a
//! trust and proxy/certificate policy that cannot be honestly tested on this
//! workstation. The CLI reports that path as an explicit HOLD rather than
//! silently treating a URL as a local path.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CompiledPack, PackError, PackSource, compile};

const LOCK_SCHEMA: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PackStoreError {
    #[error("pack store I/O at {0}: {1}")]
    Io(PathBuf, #[source] io::Error),

    #[error("pack store safe I/O failed: {0}")]
    SafeIo(#[from] frank_safeio::SafeIoError),

    #[error("could not parse pack lockfile {0}: {1}")]
    LockToml(PathBuf, #[source] Box<toml::de::Error>),

    #[error("could not serialize pack lockfile {0}: {1}")]
    LockSerialize(PathBuf, #[source] toml::ser::Error),

    #[error(transparent)]
    Compile(#[from] PackError),

    #[error("unsupported pack lock schema {0}; this binary supports schema 1")]
    UnsupportedLockSchema(u32),

    #[error("pack source is not a directory: {0}")]
    NotDirectory(PathBuf),

    #[error("pack source contains a symlink, which is not allowed: {0}")]
    Symlink(PathBuf),

    #[error("pack source contains a non-regular filesystem entry: {0}")]
    NonRegular(PathBuf),

    #[error("pack {0} is already installed")]
    AlreadyInstalled(String),

    #[error("pack {0} is not installed")]
    NotInstalled(String),

    #[error("pack selector '{0}' is ambiguous; include the version as id@version")]
    Ambiguous(String),

    #[error("pack identifier '{0}' is not safe for a directory name")]
    InvalidIdentifier(String),

    #[error("invalid SHA-256 digest '{0}'; expected exactly 64 hexadecimal characters")]
    InvalidDigest(String),

    #[error("SHA-256 mismatch for pack source: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("locked pack {0}@{1} is missing from the pack store")]
    MissingLockedPath(String, String),

    #[error(
        "locked pack {id}@{version} changed after installation: expected {expected}, got {actual}"
    )]
    LockedDigestMismatch {
        id: String,
        version: String,
        expected: String,
        actual: String,
    },

    #[error("active pack {0}@{1} is not present in packs.lock")]
    MissingActive(String, String),

    #[error("installed pack path is not safe: {0}")]
    InvalidLockedPath(String),
}

pub type StoreResult<T> = std::result::Result<T, PackStoreError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackRef {
    pub id: String,
    pub version: String,
}

impl PackRef {
    pub fn display_name(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPack {
    pub id: String,
    pub version: String,
    /// Relative to the Frank data root, normally `packs/<id>@<version>`.
    pub path: String,
    pub sha256: String,
    pub source: String,
}

impl InstalledPack {
    pub fn pack_ref(&self) -> PackRef {
        PackRef {
            id: self.id.clone(),
            version: self.version.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackLock {
    pub schema: u32,
    #[serde(default)]
    pub active: Option<PackRef>,
    #[serde(default)]
    pub packs: Vec<InstalledPack>,
}

impl Default for PackLock {
    fn default() -> Self {
        Self {
            schema: LOCK_SCHEMA,
            active: None,
            packs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackInstall {
    pub pack: CompiledPack,
    pub installed: InstalledPack,
}

#[derive(Debug, Clone)]
pub struct PackStore {
    root: PathBuf,
}

impl PackStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join("packs.lock")
    }

    pub fn load_lock(&self) -> StoreResult<PackLock> {
        let path = self.lock_path();
        let raw = match frank_safeio::read_text_capped(&path, frank_safeio::MAX_CONFIG_BYTES) {
            Ok(raw) => raw,
            Err(frank_safeio::SafeIoError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(PackLock::default());
            }
            Err(e) => return Err(PackStoreError::SafeIo(e)),
        };
        let lock: PackLock = toml::from_str(&raw)
            .map_err(|e| PackStoreError::LockToml(path.clone(), Box::new(e)))?;
        if lock.schema != LOCK_SCHEMA {
            return Err(PackStoreError::UnsupportedLockSchema(lock.schema));
        }
        validate_lock(&lock)?;
        Ok(lock)
    }

    pub fn save_lock(&self, lock: &PackLock) -> StoreResult<()> {
        validate_lock(lock)?;
        let path = self.lock_path();
        frank_safeio::ensure_dir(&self.root)?;
        let raw = toml::to_string_pretty(lock)
            .map_err(|e| PackStoreError::LockSerialize(path.clone(), e))?;
        frank_safeio::write_text_atomic(&path, &raw, frank_safeio::MAX_CONFIG_BYTES)?;
        Ok(())
    }

    /// Compile and install a local directory. The source is copied into a
    /// store-owned directory, so later changes to the author's working tree
    /// cannot change what the lockfile claims is active.
    pub fn add_local(
        &self,
        source: &Path,
        expected_sha256: Option<&str>,
    ) -> StoreResult<PackInstall> {
        let metadata = fs::symlink_metadata(source)
            .map_err(|e| PackStoreError::Io(source.to_path_buf(), e))?;
        if metadata.file_type().is_symlink() {
            return Err(PackStoreError::Symlink(source.to_path_buf()));
        }
        if !metadata.is_dir() {
            return Err(PackStoreError::NotDirectory(source.to_path_buf()));
        }
        let source =
            fs::canonicalize(source).map_err(|e| PackStoreError::Io(source.to_path_buf(), e))?;
        validate_tree(&source)?;

        let source_pack = PackSource::load(&source)?;
        let compiled = compile(&source_pack)?;
        validate_component(&compiled.id)?;
        validate_component(&compiled.version)?;

        let digest = directory_sha256(&source)?;
        if let Some(expected) = expected_sha256 {
            let expected = normalize_digest(expected)?;
            if expected != digest {
                return Err(PackStoreError::DigestMismatch {
                    expected,
                    actual: digest,
                });
            }
        }

        let mut lock = self.load_lock()?;
        let pack_ref = PackRef {
            id: compiled.id.clone(),
            version: compiled.version.clone(),
        };
        if lock.packs.iter().any(|p| p.pack_ref() == pack_ref) {
            return Err(PackStoreError::AlreadyInstalled(pack_ref.display_name()));
        }

        let relative = PathBuf::from("packs").join(pack_ref.display_name());
        let destination = self.root.join(&relative);
        if fs::symlink_metadata(&destination).is_ok() {
            return Err(PackStoreError::AlreadyInstalled(pack_ref.display_name()));
        }
        frank_safeio::ensure_dir(destination.parent().expect("packs has a parent"))?;
        let staging = create_staging_dir(&destination)?;
        if let Err(e) = copy_tree(&source, &staging) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        let staged_digest = match directory_sha256(&staging) {
            Ok(digest) => digest,
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
        };
        if staged_digest != digest {
            let _ = fs::remove_dir_all(&staging);
            return Err(PackStoreError::DigestMismatch {
                expected: digest,
                actual: staged_digest,
            });
        }
        if fs::symlink_metadata(&destination).is_ok() {
            let _ = fs::remove_dir_all(&staging);
            return Err(PackStoreError::AlreadyInstalled(pack_ref.display_name()));
        }
        if let Err(e) = fs::rename(&staging, &destination) {
            let _ = fs::remove_dir_all(&staging);
            return Err(PackStoreError::Io(destination.clone(), e));
        }

        let installed = InstalledPack {
            id: compiled.id.clone(),
            version: compiled.version.clone(),
            path: relative.to_string_lossy().replace('\\', "/"),
            sha256: digest,
            source: source.to_string_lossy().into_owned(),
        };
        lock.packs.push(installed.clone());
        if let Err(e) = self.save_lock(&lock) {
            let _ = fs::remove_dir_all(&destination);
            return Err(e);
        }
        Ok(PackInstall {
            pack: compiled,
            installed,
        })
    }

    pub fn find(&self, selector: &str) -> StoreResult<InstalledPack> {
        let lock = self.load_lock()?;
        let mut matches = lock.packs.iter().filter(|p| matches_selector(p, selector));
        let Some(first) = matches.next() else {
            return Err(PackStoreError::NotInstalled(selector.to_string()));
        };
        if matches.next().is_some() {
            return Err(PackStoreError::Ambiguous(selector.to_string()));
        }
        Ok(first.clone())
    }

    pub fn compile_installed(&self, installed: &InstalledPack) -> StoreResult<CompiledPack> {
        let expected_digest = validate_installed_pack(installed)?;
        let path = self.root.join(&installed.path);
        let metadata = fs::symlink_metadata(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                PackStoreError::MissingLockedPath(installed.id.clone(), installed.version.clone())
            } else {
                PackStoreError::Io(path.clone(), e)
            }
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PackStoreError::MissingLockedPath(
                installed.id.clone(),
                installed.version.clone(),
            ));
        }
        validate_tree(&path)?;
        let actual = directory_sha256(&path)?;
        if actual != expected_digest {
            return Err(PackStoreError::LockedDigestMismatch {
                id: installed.id.clone(),
                version: installed.version.clone(),
                expected: expected_digest,
                actual,
            });
        }
        Ok(compile(&PackSource::load(&path)?)?)
    }

    pub fn active(&self) -> StoreResult<Option<(PackRef, CompiledPack)>> {
        let lock = self.load_lock()?;
        let Some(active) = lock.active else {
            return Ok(None);
        };
        let Some(installed) = lock.packs.iter().find(|p| p.pack_ref() == active) else {
            return Err(PackStoreError::MissingActive(active.id, active.version));
        };
        Ok(Some((active, self.compile_installed(installed)?)))
    }

    pub fn set_active(&self, active: Option<PackRef>) -> StoreResult<()> {
        let mut lock = self.load_lock()?;
        if let Some(ref selected) = active {
            if !lock.packs.iter().any(|p| p.pack_ref() == *selected) {
                return Err(PackStoreError::NotInstalled(selected.display_name()));
            }
        }
        lock.active = active;
        self.save_lock(&lock)
    }

    pub fn remove(&self, selector: &str) -> StoreResult<InstalledPack> {
        let mut lock = self.load_lock()?;
        let index = {
            let mut found = None;
            for (i, pack) in lock.packs.iter().enumerate() {
                if matches_selector(pack, selector) {
                    if found.is_some() {
                        return Err(PackStoreError::Ambiguous(selector.to_string()));
                    }
                    found = Some(i);
                }
            }
            found.ok_or_else(|| PackStoreError::NotInstalled(selector.to_string()))?
        };
        let removed = lock.packs.remove(index);
        let path = self.installed_path(&removed)?;
        if lock.active.as_ref() == Some(&removed.pack_ref()) {
            lock.active = None;
        }
        // Commit the lock first. If this fails, the installed directory is
        // still referenced and can be retried; deleting it first would leave
        // a lockfile pointing at missing data.
        self.save_lock(&lock)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(PackStoreError::MissingLockedPath(
                    removed.id,
                    removed.version,
                ));
            }
            Ok(_) => fs::remove_dir_all(&path).map_err(|e| PackStoreError::Io(path.clone(), e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(PackStoreError::Io(path, e)),
        }
        Ok(removed)
    }

    fn installed_path(&self, installed: &InstalledPack) -> StoreResult<PathBuf> {
        validate_installed_pack(installed)?;
        Ok(self.root.join(&installed.path))
    }
}

fn matches_selector(pack: &InstalledPack, selector: &str) -> bool {
    selector == pack.id || selector == format!("{}@{}", pack.id, pack.version)
}

fn validate_component(value: &str) -> StoreResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(PackStoreError::InvalidIdentifier(value.to_string()));
    }
    Ok(())
}

fn validate_installed_pack(installed: &InstalledPack) -> StoreResult<String> {
    validate_component(&installed.id)?;
    validate_component(&installed.version)?;
    let digest = normalize_digest(&installed.sha256)?;
    let expected_path = format!("packs/{}", installed.pack_ref().display_name());
    if installed.path != expected_path {
        return Err(PackStoreError::InvalidLockedPath(installed.path.clone()));
    }
    Ok(digest)
}

fn validate_lock(lock: &PackLock) -> StoreResult<()> {
    if let Some(active) = &lock.active {
        validate_component(&active.id)?;
        validate_component(&active.version)?;
    }
    for installed in &lock.packs {
        validate_installed_pack(installed)?;
    }
    Ok(())
}

fn create_staging_dir(destination: &Path) -> StoreResult<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    create_staging_dir_with_stamp(destination, stamp)
}

fn create_staging_dir_with_stamp(destination: &Path, stamp: u128) -> StoreResult<PathBuf> {
    let parent = destination
        .parent()
        .expect("a pack destination always has a parent");
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pack");

    for attempt in 0..16 {
        let candidate = parent.join(format!(
            ".{name}.tmp-{}-{stamp}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(PackStoreError::Io(candidate, e)),
        }
    }

    Err(PackStoreError::Io(
        parent.to_path_buf(),
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate pack staging directory",
        ),
    ))
}

fn validate_tree(root: &Path) -> StoreResult<()> {
    let metadata =
        fs::symlink_metadata(root).map_err(|e| PackStoreError::Io(root.to_path_buf(), e))?;
    if metadata.file_type().is_symlink() {
        return Err(PackStoreError::Symlink(root.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(PackStoreError::NotDirectory(root.to_path_buf()));
    }
    for entry in fs::read_dir(root).map_err(|e| PackStoreError::Io(root.to_path_buf(), e))? {
        let entry = entry.map_err(|e| PackStoreError::Io(root.to_path_buf(), e))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| PackStoreError::Io(path.clone(), e))?;
        if metadata.file_type().is_symlink() {
            return Err(PackStoreError::Symlink(path));
        }
        if metadata.is_dir() {
            validate_tree(&path)?;
        } else if !metadata.is_file() {
            return Err(PackStoreError::NonRegular(path));
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> StoreResult<()> {
    fs::create_dir_all(destination)
        .map_err(|e| PackStoreError::Io(destination.to_path_buf(), e))?;
    for entry in fs::read_dir(source).map_err(|e| PackStoreError::Io(source.to_path_buf(), e))? {
        let entry = entry.map_err(|e| PackStoreError::Io(source.to_path_buf(), e))?;
        let source_path = entry.path();
        let target_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|e| PackStoreError::Io(source_path.clone(), e))?;
        if metadata.file_type().is_symlink() {
            return Err(PackStoreError::Symlink(source_path));
        }
        if metadata.is_dir() {
            copy_tree(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &target_path).map_err(|e| PackStoreError::Io(target_path, e))?;
        } else {
            return Err(PackStoreError::NonRegular(source_path));
        }
    }
    Ok(())
}

/// Stable digest over relative POSIX paths and file bytes. Directory mtimes,
/// permissions and traversal order intentionally do not affect the result.
pub fn directory_sha256(root: &Path) -> StoreResult<String> {
    validate_tree(root)?;
    let mut files = Vec::new();
    collect_files(root, Path::new(""), &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path).map_err(|e| PackStoreError::Io(path, e))?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    Ok(format_digest(hasher.finalize().as_slice()))
}

fn collect_files(
    root: &Path,
    relative: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> StoreResult<()> {
    for entry in
        fs::read_dir(root.join(relative)).map_err(|e| PackStoreError::Io(root.join(relative), e))?
    {
        let entry = entry.map_err(|e| PackStoreError::Io(root.join(relative), e))?;
        let name = entry.file_name();
        let rel = relative.join(&name);
        let path = root.join(&rel);
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| PackStoreError::Io(path.clone(), e))?;
        if metadata.file_type().is_symlink() {
            return Err(PackStoreError::Symlink(path));
        }
        if metadata.is_dir() {
            collect_files(root, &rel, out)?;
        } else if metadata.is_file() {
            let normalized = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            out.push((normalized, path));
        } else {
            return Err(PackStoreError::NonRegular(path));
        }
    }
    Ok(())
}

fn normalize_digest(value: &str) -> StoreResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PackStoreError::InvalidDigest(value.to_string()));
    }
    Ok(normalized)
}

fn format_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_pack(root: &Path, id: &str, version: &str) {
        fs::create_dir_all(root.join("levels")).unwrap();
        fs::write(
            root.join("pack.toml"),
            format!(
                "schema = 1\n\n[pack]\nid = \"{id}\"\nversion = \"{version}\"\ndefault_level = \"full\"\n\n[[level]]\nid = \"full\"\ncompose = [\"@rules\"]\nrules = \"levels/full.md\"\n"
            ),
        )
        .unwrap();
        fs::write(root.join("levels/full.md"), "Be concise.").unwrap();
    }

    #[test]
    fn add_compile_select_and_remove_local_pack() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        let store_root = tmp.path().join("data/frank");
        write_pack(&source, "demo", "1.0.0");
        let store = PackStore::new(store_root);

        let added = store.add_local(&source, None).unwrap();
        assert_eq!(added.installed.pack_ref().display_name(), "demo@1.0.0");
        assert!(store.lock_path().is_file());
        assert!(store.find("demo").is_ok());

        store.set_active(Some(added.installed.pack_ref())).unwrap();
        let (_, active) = store.active().unwrap().unwrap();
        assert_eq!(active.id, "demo");
        assert_eq!(
            active.resolve_level("full").unwrap().activation_prompt,
            "Be concise."
        );

        store.remove("demo").unwrap();
        assert!(store.active().unwrap().is_none());
    }

    #[test]
    fn digest_is_order_independent() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("z")).unwrap();
        fs::write(tmp.path().join("b"), "two").unwrap();
        fs::write(tmp.path().join("z/a"), "one").unwrap();
        let first = directory_sha256(tmp.path()).unwrap();
        fs::write(tmp.path().join("a"), "not part of the first digest").unwrap();
        let second = directory_sha256(tmp.path()).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn changed_locked_copy_is_refused() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        write_pack(&source, "demo", "1.0.0");
        let store = PackStore::new(tmp.path().join("data/frank"));
        let added = store.add_local(&source, None).unwrap();
        let locked_file = store
            .root()
            .join(&added.installed.path)
            .join("levels/full.md");
        fs::write(locked_file, "tampered").unwrap();

        let err = store.compile_installed(&added.installed).unwrap_err();
        assert!(matches!(err, PackStoreError::LockedDigestMismatch { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_pack_entries_are_rejected() {
        use std::os::unix::fs::symlink;
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        write_pack(&source, "demo", "1.0.0");
        symlink(tmp.path().join("outside"), source.join("link")).unwrap();
        let err = PackStore::new(tmp.path().join("data"))
            .add_local(&source, None)
            .unwrap_err();
        assert!(matches!(err, PackStoreError::Symlink(_)));
    }

    #[cfg(unix)]
    #[test]
    fn nested_symlinked_pack_directory_is_rejected_before_reading_outside() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        write_pack(&source, "demo", "1.0.0");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("full.md"), "outside prompt").unwrap();
        fs::remove_dir_all(source.join("levels")).unwrap();
        symlink(&outside, source.join("levels")).unwrap();

        let error = crate::compile(&crate::PackSource::load(&source).unwrap()).unwrap_err();
        assert!(
            matches!(error, crate::PackError::UnsafePath(_)),
            "{error:?}"
        );
    }

    #[test]
    fn lock_paths_must_stay_inside_the_pack_store() {
        let tmp = tempdir().unwrap();
        let store = PackStore::new(tmp.path().join("data"));
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(store.root()).unwrap();
        let lock = PackLock {
            schema: 1,
            active: None,
            packs: vec![InstalledPack {
                id: "demo".into(),
                version: "1.0.0".into(),
                path: "../outside".into(),
                sha256: "0".repeat(64),
                source: "local".into(),
            }],
        };
        fs::write(store.lock_path(), toml::to_string(&lock).unwrap()).unwrap();

        let err = store.remove("demo").unwrap_err();
        assert!(matches!(err, PackStoreError::InvalidLockedPath(_)));
        assert!(
            outside.is_dir(),
            "tampered lock must not remove outside data"
        );
    }

    #[cfg(unix)]
    #[test]
    fn lockfile_symlink_is_rejected_fail_closed() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let root = tmp.path().join("data");
        fs::create_dir_all(&root).unwrap();
        let decoy = tmp.path().join("decoy.toml");
        fs::write(&decoy, "schema = 1\n").unwrap();
        symlink(&decoy, root.join("packs.lock")).unwrap();

        let error = PackStore::new(root).load_lock().unwrap_err();
        assert!(matches!(error, PackStoreError::SafeIo(_)));
        assert_eq!(fs::read_to_string(decoy).unwrap(), "schema = 1\n");
    }

    #[test]
    fn pack_selectors_and_components_are_exact_and_safe() {
        let pack = InstalledPack {
            id: "demo".into(),
            version: "1.0.0".into(),
            path: "packs/demo@1.0.0".into(),
            sha256: "0".repeat(64),
            source: "local".into(),
        };
        assert!(matches_selector(&pack, "demo"));
        assert!(matches_selector(&pack, "demo@1.0.0"));
        assert!(!matches_selector(&pack, "demo@2.0.0"));
        assert!(!matches_selector(&pack, "dem"));

        for invalid in ["", ".", "..", "bad/name"] {
            assert!(matches!(
                validate_component(invalid),
                Err(PackStoreError::InvalidIdentifier(value)) if value == invalid
            ));
        }
        validate_component("safe-name_1.0").unwrap();
    }

    #[test]
    fn compile_installed_rejects_a_locked_file_instead_of_treating_it_as_a_pack() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("data");
        fs::create_dir_all(root.join("packs")).unwrap();
        fs::write(root.join("packs/demo@1.0.0"), "not a directory").unwrap();
        let installed = InstalledPack {
            id: "demo".into(),
            version: "1.0.0".into(),
            path: "packs/demo@1.0.0".into(),
            sha256: "0".repeat(64),
            source: "local".into(),
        };

        assert!(matches!(
            PackStore::new(root).compile_installed(&installed),
            Err(PackStoreError::MissingLockedPath(_, _))
        ));
    }

    #[test]
    fn digest_mismatch_and_duplicate_destinations_are_rejected() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        write_pack(&source, "demo", "1.0.0");
        let store = PackStore::new(tmp.path().join("data"));

        assert!(matches!(
            store.add_local(&source, Some(&"0".repeat(64))),
            Err(PackStoreError::DigestMismatch { .. })
        ));

        store.add_local(&source, None).unwrap();
        assert!(matches!(
            store.add_local(&source, None),
            Err(PackStoreError::AlreadyInstalled(name)) if name == "demo@1.0.0"
        ));

        let empty_store = PackStore::new(tmp.path().join("other-data"));
        fs::create_dir_all(empty_store.root().join("packs/demo@1.0.0")).unwrap();
        assert!(matches!(
            empty_store.add_local(&source, None),
            Err(PackStoreError::AlreadyInstalled(name)) if name == "demo@1.0.0"
        ));

        let locked_store = PackStore::new(tmp.path().join("locked-data"));
        locked_store
            .save_lock(&PackLock {
                schema: 1,
                active: None,
                packs: vec![InstalledPack {
                    id: "demo".into(),
                    version: "1.0.0".into(),
                    path: "packs/demo@1.0.0".into(),
                    sha256: directory_sha256(&source).unwrap(),
                    source: "local".into(),
                }],
            })
            .unwrap();
        assert!(matches!(
            locked_store.add_local(&source, None),
            Err(PackStoreError::AlreadyInstalled(name)) if name == "demo@1.0.0"
        ));
    }

    #[test]
    fn locked_missing_and_non_directory_paths_fail_closed() {
        let tmp = tempdir().unwrap();
        let installed = InstalledPack {
            id: "demo".into(),
            version: "1.0.0".into(),
            path: "packs/demo@1.0.0".into(),
            sha256: "0".repeat(64),
            source: "local".into(),
        };

        let missing_root = tmp.path().join("missing");
        fs::create_dir_all(missing_root.join("packs")).unwrap();
        assert!(matches!(
            PackStore::new(missing_root).compile_installed(&installed),
            Err(PackStoreError::MissingLockedPath(_, _))
        ));

        let blocked_root = tmp.path().join("blocked");
        fs::create_dir_all(&blocked_root).unwrap();
        fs::write(blocked_root.join("packs"), "not a directory").unwrap();
        assert!(matches!(
            PackStore::new(blocked_root).compile_installed(&installed),
            Err(PackStoreError::Io(_, _))
        ));
    }

    #[test]
    fn remove_handles_missing_directory_and_rejects_a_locked_file() {
        let tmp = tempdir().unwrap();
        let store = PackStore::new(tmp.path().join("data"));
        let missing = InstalledPack {
            id: "missing".into(),
            version: "1.0.0".into(),
            path: "packs/missing@1.0.0".into(),
            sha256: "0".repeat(64),
            source: "local".into(),
        };
        store
            .save_lock(&PackLock {
                schema: 1,
                active: None,
                packs: vec![missing],
            })
            .unwrap();
        assert!(store.remove("missing").is_ok());
        assert!(store.load_lock().unwrap().packs.is_empty());

        let file_store = PackStore::new(tmp.path().join("file-data"));
        let locked = InstalledPack {
            id: "file".into(),
            version: "1.0.0".into(),
            path: "packs/file@1.0.0".into(),
            sha256: "0".repeat(64),
            source: "local".into(),
        };
        fs::create_dir_all(file_store.root().join("packs")).unwrap();
        fs::write(file_store.root().join(&locked.path), "not a directory").unwrap();
        file_store
            .save_lock(&PackLock {
                schema: 1,
                active: None,
                packs: vec![locked],
            })
            .unwrap();
        assert!(matches!(
            file_store.remove("file"),
            Err(PackStoreError::MissingLockedPath(_, _))
        ));
    }

    #[test]
    fn a_directory_lockfile_is_not_treated_as_missing() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("data");
        fs::create_dir_all(root.join("packs.lock")).unwrap();
        assert!(matches!(
            PackStore::new(root).load_lock(),
            Err(PackStoreError::SafeIo(_))
        ));
    }

    #[test]
    fn invalid_utf8_lockfiles_are_not_treated_as_missing() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("data");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("packs.lock"), [0xff, 0xfe]).unwrap();
        assert!(matches!(
            PackStore::new(root).load_lock(),
            Err(PackStoreError::SafeIo(_))
        ));
    }

    #[test]
    fn staging_directory_retries_only_existing_candidates() {
        let tmp = tempdir().unwrap();
        let destination = tmp.path().join("packs/demo@1.0.0");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        let stamp = 42_u128;
        let candidate = destination
            .parent()
            .unwrap()
            .join(format!(".demo@1.0.0.tmp-{}-{stamp}-0", std::process::id()));
        fs::create_dir(&candidate).unwrap();
        let created = create_staging_dir_with_stamp(&destination, stamp).unwrap();
        assert!(
            created
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-1")
        );
        fs::remove_dir_all(created).unwrap();

        let blocked_destination = tmp.path().join("missing-parent/demo@1.0.0");
        let error = create_staging_dir_with_stamp(&blocked_destination, stamp).unwrap_err();
        match error {
            PackStoreError::Io(path, _) => {
                assert!(path.file_name().unwrap().to_string_lossy().ends_with("-0"))
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn remove_does_not_swallow_non_not_found_errors() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("blocked-data");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("packs"), "not a directory").unwrap();
        let store = PackStore::new(root);
        store
            .save_lock(&PackLock {
                schema: 1,
                active: None,
                packs: vec![InstalledPack {
                    id: "blocked".into(),
                    version: "1.0.0".into(),
                    path: "packs/blocked@1.0.0".into(),
                    sha256: "0".repeat(64),
                    source: "local".into(),
                }],
            })
            .unwrap();
        assert!(matches!(
            store.remove("blocked"),
            Err(PackStoreError::Io(_, _))
        ));
    }
}
