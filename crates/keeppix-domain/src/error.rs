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
    #[error("invalid rating: {0}")]
    InvalidRating(u8),
    #[error("invalid pick: {0}")]
    InvalidPick(String),
    #[error("invalid disk action: {0}")]
    InvalidDiskAction(String),
    #[error("invalid operation kind: {0}")]
    InvalidOperationKind(String),
    #[error("invalid operation status: {0}")]
    InvalidOperationStatus(String),
    #[error("invalid tag kind: {0}")]
    InvalidTagKind(String),
}
