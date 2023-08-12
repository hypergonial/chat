use s3::error::S3Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChatError {
    #[error("Database transaction failed: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("S3 error: {0}")]
    S3Error(#[from] S3Error),
}
