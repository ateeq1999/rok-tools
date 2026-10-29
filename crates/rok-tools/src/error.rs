//! The single error type every Tool returns.

use std::path::PathBuf;

/// Why a Tool could not produce its output.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// A filesystem operation failed.
    #[error("{action} {path}: {source}")]
    Io {
        /// What was being attempted (e.g. "read", "write").
        action: &'static str,
        /// The path the operation was applied to.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A path the Tool needs does not exist.
    #[error("not found: {}", .0.display())]
    NotFound(PathBuf),
    /// The options were invalid (bad pattern, missing argument, unparsable value…).
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    /// An external command could not be started or reported failure.
    #[error("command `{command}` failed: {message}")]
    Command {
        /// The program that was run.
        command: String,
        /// What went wrong.
        message: String,
    },
    /// The Tool ran but could not complete for another reason.
    #[error("{0}")]
    Failed(String),
    /// A Payload asked for a Tool that this build does not include.
    #[error("tool `{0}` not enabled (enable the `{0}` feature of rok-tools)")]
    NotEnabled(String),
    /// A Payload Step needs a permission the Payload did not grant.
    #[error("tool `{tool}` requires `{permission}: true` in the Payload options")]
    PermissionDenied {
        /// The Tool that was refused.
        tool: String,
        /// The Payload option that grants it.
        permission: &'static str,
    },
}

impl ToolError {
    /// Wraps an I/O error with the action and path it came from.
    pub fn io(action: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        let path = path.into();
        if source.kind() == std::io::ErrorKind::NotFound {
            return Self::NotFound(path);
        }
        Self::Io {
            action,
            path,
            source,
        }
    }

    /// Builds an [`ToolError::InvalidOptions`] from anything displayable.
    pub fn invalid(message: impl std::fmt::Display) -> Self {
        Self::InvalidOptions(message.to_string())
    }
}

/// Result alias used throughout the crate.
pub type Result<T, E = ToolError> = std::result::Result<T, E>;
