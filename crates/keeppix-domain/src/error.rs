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
}
