use thiserror::Error;

#[derive(Debug, Error)]
pub enum JobError {
    #[error("database: {0}")]
    Db(#[from] keeppix_db::DbError),
    #[error("mass disappearance")]
    MassDisappearance,
    #[error("worker: {0}")]
    Worker(String),
}
