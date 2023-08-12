use std::num::ParseIntError;

use derive_builder::UninitializedFieldError;
use s3::error::S3Error;
use thiserror::Error;

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum BuilderError {
    #[error("Uninitialized field: {0}")]
    UninitializedField(&'static str),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

impl From<UninitializedFieldError> for BuilderError {
    fn from(e: UninitializedFieldError) -> Self {
        Self::UninitializedField(e.field_name())
    }
}

impl From<String> for BuilderError {
    fn from(e: String) -> Self {
        Self::ValidationError(e)
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ChatError {
    #[error("Database transaction failed: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("S3 error: {0}")]
    S3Error(#[from] S3Error),
    #[error("Failed to serialize/deserialize JSON: {0}")]
    JSONError(#[from] serde_json::Error),
    #[error("Failed to match regex: {0}")]
    RegexError(#[from] regex::Error),
    #[error("Missing field from request: {0}")]
    MissingFieldError(String),
    #[error("Malformed field: {0}")]
    MalformedFieldError(String),
    #[error("Failed to build object: {0}")]
    BuilderError(#[from] BuilderError),
    #[error("Failed to parse int: {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
