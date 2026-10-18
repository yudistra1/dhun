//! Errors from loading, saving, and validating projects.

use thiserror::Error;

/// Something went wrong reading, writing, parsing, or validating a project.
///
/// Malformed input must always surface here, never as a panic (SPEC §5, §8.4).
#[derive(Debug, Error)]
pub enum ProjectError {
    /// A filesystem error while loading or saving.
    #[error("project I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The project JSON could not be parsed or did not match the schema.
    #[error("project parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// The project's `format_version` is newer than this build understands.
    #[error("unsupported project format version {found} (this build supports up to {supported})")]
    UnsupportedVersion {
        /// The version found in the file.
        found: u32,
        /// The newest version this build can read.
        supported: u32,
    },

    /// The project parsed but is internally inconsistent or out of range.
    #[error("invalid project: {0}")]
    Validation(String),
}
