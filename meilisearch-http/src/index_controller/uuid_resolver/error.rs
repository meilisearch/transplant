use meilisearch_error::{Code, ErrorCode};

pub type Result<T> = std::result::Result<T, UuidResolverError>;

#[derive(Debug, thiserror::Error)]
pub enum UuidResolverError {
    #[error("name already exist.")]
    NameAlreadyExist,
    #[error("index \"{0}\" doesn't exist.")]
    UnexistingIndex(String),
    #[error("badly formatted index uid: {0}")]
    BadlyFormatted(String),
    #[error("internal error resolving index uid: {0}")]
    Internal(Box<dyn std::error::Error + Sync + Send + 'static>),
}

internal_error!(
    UuidResolverError: heed::Error,
    uuid::Error,
    std::io::Error,
    tokio::task::JoinError,
    serde_json::Error
);

impl ErrorCode for UuidResolverError {
    fn error_code(&self) -> Code {
        match self {
            UuidResolverError::NameAlreadyExist => Code::IndexAlreadyExists,
            UuidResolverError::UnexistingIndex(_) => Code::IndexNotFound,
            UuidResolverError::BadlyFormatted(_) => Code::InvalidIndexUid,
            UuidResolverError::Internal(_) => Code::Internal,
        }
    }
}