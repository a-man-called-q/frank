//! Deterministic prose compressor, validator, and file classifier.
//!
//! Three independent pieces: [`classify`] decides whether a file is worth
//! compressing at all, [`rules`] does the actual compression, and
//! [`validate`] checks a compression didn't lose anything it shouldn't
//! have. See each module's docs for what was ported from the archive
//! verbatim versus deliberately redesigned.

pub mod backup;
pub mod classify;
pub mod frontmatter;
pub mod rules;
pub mod sensitive;
pub mod validate;

pub use backup::{backup_dir_for, backup_path_for};
pub use classify::{detect_file_type, should_compress, FileClass};
pub use frontmatter::split_frontmatter;
pub use rules::{compress, compress_prose, protected_spans, CompressResult};
pub use sensitive::is_sensitive_path;
pub use validate::{validate, ValidationResult};

#[cfg(test)]
mod classify_tests;
#[cfg(test)]
mod rules_tests;
#[cfg(test)]
mod safety_tests;
#[cfg(test)]
mod validate_tests;
