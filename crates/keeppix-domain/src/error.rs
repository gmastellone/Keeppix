use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid username: {0}")]
    InvalidUsername(String),
    #[error("invalid password: {0}")]
    InvalidPassword(String),
    #[error("password hashing failed: {0}")]
    PasswordHashing(String),
    #[error("invalid folder path: {0}")]
    InvalidFolderPath(String),
    #[error("invalid asset name: {0}")]
    InvalidAssetName(String),
    #[error("invalid job kind: {0}")]
    InvalidJobKind(String),
    #[error("invalid job status: {0}")]
    InvalidJobStatus(String),
    #[error("invalid job priority: {0}")]
    InvalidJobPriority(i16),
}
