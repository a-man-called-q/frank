//! Server-owned Markdown memories. No embeddings/vector store in v1.

use std::path::{Path, PathBuf};

use frank_protocol::AgentId;
use frank_safeio::{MAX_CONFIG_BYTES, SafeIoError, read_text_capped, write_text_atomic};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("memory path is outside the agent scope")]
    InvalidPath,
    #[error("memory is too large")]
    TooLarge,
    #[error("memory IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("safe memory IO failed: {0}")]
    SafeIo(#[from] SafeIoError),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

#[derive(Debug, Clone)]
pub struct MemoryRepository {
    pub root: PathBuf,
    pub max_bytes: usize,
}

impl MemoryRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_bytes: MAX_CONFIG_BYTES,
        }
    }

    pub fn path_for(&self, agent_id: AgentId, relative: &str) -> Result<PathBuf> {
        if relative
            .split(['/', '\\'])
            .any(|component| component == "..")
            || Path::new(relative).is_absolute()
            || relative.starts_with('\\')
            || !relative.ends_with("memory.md")
        {
            return Err(MemoryError::InvalidPath);
        }
        Ok(self.root.join(agent_id.to_string()).join(relative))
    }

    pub fn propose(&self, agent_id: AgentId, relative: &str, content: &str) -> Result<PathBuf> {
        if content.len() > self.max_bytes {
            return Err(MemoryError::TooLarge);
        }
        let path = self.path_for(agent_id, relative)?;
        let parent = path.parent().ok_or(MemoryError::InvalidPath)?;
        frank_safeio::ensure_dir(parent)?;
        write_text_atomic(&path, content, self.max_bytes)?;
        Ok(path)
    }

    pub fn read(&self, agent_id: AgentId, relative: &str) -> Result<Option<String>> {
        let path = self.path_for(agent_id, relative)?;
        match read_text_capped(&path, self.max_bytes) {
            Ok(content) => Ok(Some(content)),
            Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(MemoryError::SafeIo(error)),
        }
    }

    pub fn archive_and_replace(
        &self,
        agent_id: AgentId,
        relative: &str,
        replacement: &str,
    ) -> Result<PathBuf> {
        if replacement.len() > self.max_bytes {
            return Err(MemoryError::TooLarge);
        }
        let path = self.path_for(agent_id, relative)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_type().is_symlink()
        {
            return Err(MemoryError::InvalidPath);
        }
        if path.is_file() {
            let archive = archive_path(&path);
            std::fs::rename(&path, &archive)?;
        }
        let parent = path.parent().ok_or(MemoryError::InvalidPath)?;
        frank_safeio::ensure_dir(parent)?;
        write_text_atomic(&path, replacement, self.max_bytes)?;
        Ok(path)
    }
}

fn archive_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("memory.md");
    path.with_file_name(format!(
        "{name}.archive-{}-{}",
        frank_protocol::timestamp_now(),
        uuid::Uuid::new_v4().simple()
    ))
}
