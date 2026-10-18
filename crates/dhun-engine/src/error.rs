//! Engine error type.

use thiserror::Error;

/// Something went wrong building or running a render.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The project failed validation before rendering.
    #[error(transparent)]
    Project(#[from] dhun_project::ProjectError),

    /// The requested render range was empty or inverted.
    #[error("invalid render range: start bar {start} is not before end bar {end}")]
    BadRange {
        /// Requested start bar.
        start: u32,
        /// Requested end bar.
        end: u32,
    },
}
