use thiserror::Error;

#[derive(Error, Debug)]
pub enum FormsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML parsing error: {0}")]
    Xml(String),

    #[error("Binary decoding error: {0}")]
    Binary(String),

    #[error("Module not found: {0}")]
    NotFound(String),

    #[error("Invalid module format: {0}")]
    InvalidFormat(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, FormsError>;
