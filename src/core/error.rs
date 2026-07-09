//! Error types for the unified processor

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessorError {
    #[error("Document processing error: {0}")]
    DocumentError(String),

    #[error("Code analysis error: {0}")]
    CodeAnalysisError(String),

    #[error("Embedding service error: {0}")]
    EmbeddingError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Redis error: {0}")]
    RedisError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Infrastructure error: {0}")]
    InfraError(String),

    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("Kafka error: {0}")]
    KafkaError(String),

    #[error("Task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<anyhow::Error> for ProcessorError {
    fn from(e: anyhow::Error) -> Self {
        ProcessorError::InfraError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ProcessorError>;
