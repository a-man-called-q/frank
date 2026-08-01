use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("failed to read {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),

    #[error("failed to parse {0}: {1}")]
    Toml(PathBuf, #[source] Box<toml::de::Error>),

    #[error("level '{0}' composes unknown fragment or token '{1}'")]
    UnknownFragment(String, String),

    #[error("level '{0}' inherits unknown level '{1}'")]
    UnknownParent(String, String),

    #[error("inheritance cycle detected at level '{0}'")]
    InheritanceCycle(String),

    #[error("duplicate level id '{0}'")]
    DuplicateLevelId(String),

    #[error("duplicate alias '{0}' (already used by level '{1}')")]
    DuplicateAlias(String, String),

    #[error("pack.default_level '{0}' does not name a defined level")]
    UnknownDefaultLevel(String),

    #[error(
        "level '{level}' {kind} is {actual} bytes, over the {limit}-byte budget in [pack.budget]"
    )]
    BudgetExceeded {
        level: String,
        kind: &'static str,
        limit: usize,
        actual: usize,
    },

    #[error("level '{0}' activation trigger regex is invalid: {1}")]
    InvalidRegex(String, #[source] regex::Error),

    #[error("level '{0}' references @rules but has no `rules` file (after inheritance)")]
    MissingRules(String),

    #[error("unsupported pack schema {0}; this binary supports schema 1")]
    UnsupportedSchema(u32),

    #[error("pack references an unsafe or symlinked path '{0}'")]
    UnsafePath(String),
}

pub type Result<T> = std::result::Result<T, PackError>;
