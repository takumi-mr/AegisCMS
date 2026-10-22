use thiserror::Error;

#[derive(Debug, Error)]
pub enum AegisError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Plugin runtime error: {0}")]
    PluginRuntime(String),

    /// A plugin turned the input down (`on-before-save` returned an error), as opposed to failing
    #[error("Rejected by plugin: {0}")]
    PluginRejected(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Theme error: {0}")]
    Theme(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for AegisError {
    fn from(err: serde_json::Error) -> Self {
        AegisError::Serialization(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AegisError>;
