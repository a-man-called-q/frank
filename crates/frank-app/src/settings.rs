use std::path::Path;

use crate::{AppError, UserSettings};

pub(super) fn read_settings(path: &Path) -> Result<UserSettings, AppError> {
    let raw = match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
        Ok(raw) => raw,
        Err(frank_safeio::SafeIoError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UserSettings::default());
        }
        Err(e) => return Err(AppError::SafeIo(e)),
    };
    toml::from_str(&raw).map_err(|e| AppError::Config {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

pub(super) fn write_settings(path: &Path, settings: &UserSettings) -> Result<(), AppError> {
    let mut doc = match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
        Ok(raw) => raw
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| AppError::Config {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?,
        Err(frank_safeio::SafeIoError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            toml_edit::DocumentMut::new()
        }
        Err(e) => return Err(AppError::SafeIo(e)),
    };
    match &settings.default_level {
        Some(level) => doc["default_level"] = toml_edit::value(level.clone()),
        None => {
            doc.remove("default_level");
        }
    }
    doc["gui"]["launch_at_login"] = toml_edit::value(settings.gui.launch_at_login);
    doc["gui"]["close_to_tray"] = toml_edit::value(settings.gui.close_to_tray);
    frank_safeio::write_text_atomic(path, &doc.to_string(), frank_safeio::MAX_CONFIG_BYTES)?;
    Ok(())
}
