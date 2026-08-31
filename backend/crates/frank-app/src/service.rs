//! Shared per-user `frankd` service descriptor and activation boundary.
//!
//! The CLI and any future installer front-end must use these functions rather
//! than rendering platform descriptors independently.  Rendering is pure and
//! previewable; activation is intentionally kept at the caller edge because
//! it invokes the host's service manager.

use std::path::{Path, PathBuf};

use frank_safeio::{SafeIoError, write_text_atomic};
use thiserror::Error;

pub const SERVICE_NAME: &str = "dev.frank.frankd";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub executable: PathBuf,
    pub database: PathBuf,
    pub bind: String,
    pub service_name: String,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("service IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("safe service file IO failed: {0}")]
    SafeIo(#[from] SafeIoError),
    #[error("service name is invalid")]
    InvalidName,
    #[error("frankd executable must be a regular, non-symlink file or a bare PATH command")]
    InvalidExecutable,
}

pub type Result<T> = std::result::Result<T, ServiceError>;

/// Render a launchd user agent.  This function is pure and is used by
/// previews/tests as well as the actual installer.
pub fn render_launch_agent(spec: &ServiceSpec) -> Result<String> {
    validate_spec(spec)?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{}</string>
<key>ProgramArguments</key><array><string>{}</string><string>--db</string><string>{}</string><string>--bind</string><string>{}</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
</dict></plist>
"#,
        xml_escape(&spec.service_name),
        xml_escape(&spec.executable.to_string_lossy()),
        xml_escape(&spec.database.to_string_lossy()),
        xml_escape(&spec.bind),
    ))
}

/// Render a systemd user unit with bounded restart behavior.
pub fn render_systemd_user(spec: &ServiceSpec) -> Result<String> {
    validate_spec(spec)?;
    Ok(format!(
        "# Managed by Frank: {}\n[Unit]\nDescription=Frank headless orchestrator\nAfter=network-online.target\n\n[Service]\nExecStart={} --db {} --bind {}\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n",
        spec.service_name,
        shell_escape(&spec.executable.to_string_lossy()),
        shell_escape(&spec.database.to_string_lossy()),
        shell_escape(&spec.bind),
    ))
}

/// Render a self-contained Scheduled Task installer.  `schtasks` supplies the
/// logon trigger; the PowerShell settings line adds restart-on-failure.
pub fn render_scheduled_task(spec: &ServiceSpec) -> Result<String> {
    validate_spec(spec)?;
    Ok(format!(
        concat!(
            "schtasks.exe /Create /TN \"{}\" /SC ONLOGON /TR \"\\\"{}\\\" --db \\\"{}\\\" --bind {}\" /F\r\n",
            "powershell.exe -NoProfile -NonInteractive -Command \"$settings = New-ScheduledTaskSettingsSet -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1); Set-ScheduledTask -TaskName '{}' -Settings $settings\""
        ),
        spec.service_name,
        powershell_quote(&spec.executable.to_string_lossy()),
        powershell_quote(&spec.database.to_string_lossy()),
        spec.bind,
        powershell_quote(&spec.service_name),
    ))
}

/// Return the platform descriptor filename and content without activating it.
pub fn descriptor(spec: &ServiceSpec, root: &Path) -> Result<(PathBuf, String)> {
    validate_spec(spec)?;
    if cfg!(target_os = "macos") {
        Ok((
            root.join(format!("{}.plist", spec.service_name)),
            render_launch_agent(spec)?,
        ))
    } else if cfg!(target_os = "windows") {
        Ok((
            root.join(format!("{}.cmd", spec.service_name)),
            render_scheduled_task(spec)?,
        ))
    } else {
        Ok((
            root.join(format!("{}.service", spec.service_name)),
            render_systemd_user(spec)?,
        ))
    }
}

/// Write a descriptor atomically and return its path.  This is the shared
/// prepare step; callers perform activation only after they have reviewed the
/// resulting path/content.
pub fn install_preview(spec: &ServiceSpec, root: &Path) -> Result<PathBuf> {
    let (path, content) = descriptor(spec, root)?;
    if let Some(parent) = path.parent() {
        frank_safeio::ensure_dir(parent)?;
    }
    write_text_atomic(&path, &content, 64 * 1024)?;
    Ok(path)
}

/// Return the descriptor path used by the per-user installer on this host.
/// Keeping this in the shared service module lets the daemon's diagnostics
/// and the CLI agree on what "installed" means without invoking a platform
/// service manager or duplicating path logic.
pub fn default_descriptor_path() -> PathBuf {
    let home = frank_safeio::home_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", SERVICE_NAME))
    } else if cfg!(target_os = "windows") {
        home.join(format!("{}.cmd", SERVICE_NAME))
    } else {
        home.join(".config")
            .join("systemd")
            .join("user")
            .join(format!("{}.service", SERVICE_NAME))
    }
}

/// Check whether a Frank-managed per-user descriptor exists.  Presence is a
/// conservative installation signal; the service manager's own status is
/// still the authority for whether it is currently running.
pub fn is_installed() -> bool {
    let path = default_descriptor_path();
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    frank_safeio::read_text_capped(&default_descriptor_path(), 64 * 1024)
        .is_ok_and(|contents| contents.contains(SERVICE_NAME))
}

/// Validate an executable supplied to a service descriptor. A bare command
/// such as `frankd` is intentionally allowed so the service manager can use
/// the user's PATH; an explicit path must already be a regular, non-symlink
/// file. This keeps a descriptor from silently following a swapped symlink.
pub fn validate_executable_path(path: &Path) -> Result<()> {
    let is_bare_command = path.components().count() == 1 && path.file_name().is_some();
    if is_bare_command {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| ServiceError::InvalidExecutable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ServiceError::InvalidExecutable);
    }
    Ok(())
}

fn validate_spec(spec: &ServiceSpec) -> Result<()> {
    if spec.service_name.is_empty()
        || spec.service_name.len() > 128
        || !spec.service_name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '.'
        })
    {
        return Err(ServiceError::InvalidName);
    }
    validate_text(&spec.executable.to_string_lossy())?;
    validate_text(&spec.database.to_string_lossy())?;
    validate_text(&spec.bind)
}

fn validate_text(value: &str) -> Result<()> {
    // These values are rendered into launchd XML, a systemd command, and a
    // Scheduled Task command line. XML/shell quoting covers ordinary spaces,
    // but Windows expands the following metacharacters before `frankd` sees
    // its arguments. Rejecting them for every descriptor keeps the shared
    // prepare step fail-closed on all platforms instead of relying on one
    // renderer's escaping details.
    if value.is_empty()
        || value.len() > 4_096
        || value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '"' | '&' | '|' | '<' | '>' | '^' | '%' | '!' | '`' | '$' | ';'
                )
        })
    {
        return Err(ServiceError::InvalidName);
    }
    Ok(())
}

fn powershell_quote(value: &str) -> String {
    value.replace('"', "\"\"")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._-:".contains(character))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ServiceSpec {
        ServiceSpec {
            executable: PathBuf::from("/opt/frankd"),
            database: PathBuf::from("/tmp/frank.sqlite3"),
            bind: "127.0.0.1:37465".into(),
            service_name: SERVICE_NAME.into(),
        }
    }

    #[test]
    fn all_platform_descriptors_restart_on_failure() {
        assert!(render_launch_agent(&spec()).unwrap().contains("KeepAlive"));
        assert!(
            render_systemd_user(&spec())
                .unwrap()
                .contains("Restart=on-failure")
        );
        let scheduled = render_scheduled_task(&spec()).unwrap();
        assert!(scheduled.contains("ONLOGON"));
        assert!(scheduled.contains("-RestartCount 3"));
    }

    #[test]
    fn descriptor_rejects_shell_breakout_characters() {
        let mut invalid = spec();
        invalid.bind = "127.0.0.1:37465\" & whoami".into();
        assert!(matches!(
            descriptor(&invalid, Path::new("/tmp")),
            Err(ServiceError::InvalidName)
        ));
    }

    #[test]
    fn descriptor_rejects_windows_shell_metacharacters_without_quotes() {
        for bind in [
            "127.0.0.1:37465 & whoami",
            "127.0.0.1:37465|whoami",
            "127.0.0.1:37465%PATH%",
            "127.0.0.1:37465`whoami",
            "127.0.0.1:37465;whoami",
        ] {
            let mut invalid = spec();
            invalid.bind = bind.into();
            assert!(matches!(
                render_scheduled_task(&invalid),
                Err(ServiceError::InvalidName)
            ));
        }
    }
}
