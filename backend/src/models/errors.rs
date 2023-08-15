use std::num::ParseIntError;

use aws_sdk_s3::error::{DisplayErrorContext, SdkError};
use axum::{
    extract::multipart::MultipartError,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use derive_builder::UninitializedFieldError;
use serde_json::json;
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

impl IntoResponse for BuilderError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::UninitializedField(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
        };
        if let StatusCode::INTERNAL_SERVER_ERROR = status {
            tracing::error!(error = %self);
        }
        let body = Json(json!({
            "error": self.to_string()
        }));
        (status, body).into_response()
    }
}

/// Errors that can occur in the application.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AppError {
    #[error("Database transaction failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("Failed to serialize/deserialize JSON: {0}")]
    JSON(#[from] serde_json::Error),
    #[error("Failed to parse multipart/form-data: {0}")]
    Multipart(#[from] MultipartError),
    #[error("Failed to parse JWT: {0}")]
    JWT(#[from] jsonwebtoken::errors::Error),
    #[error("Failed to match regex: {0}")]
    Regex(#[from] regex::Error),
    #[error("Failed to build object: {0}")]
    Builder(#[from] BuilderError),
    #[error("Failed to parse int: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("Authentication failure: {0}")]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::S3(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::JSON(_) => StatusCode::BAD_REQUEST,
            Self::Multipart(_) => StatusCode::BAD_REQUEST,
            Self::JWT(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Regex(_) => StatusCode::BAD_REQUEST,
            Self::Builder(e) => return e.into_response(),
            Self::ParseInt(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Auth(e) => return e.into_response(),
            Self::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if let StatusCode::INTERNAL_SERVER_ERROR = status {
            tracing::error!(error = %self);
        }
        let body = Json(json!({
            "error": self.to_string()
        }));
        (status, body).into_response()
    }
}

/// Hacky workaround for SdkError having a generic type parameter
impl<E, R> From<SdkError<E, R>> for AppError
where
    E: std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug,
{
    fn from(e: SdkError<E, R>) -> Self {
        Self::S3(DisplayErrorContext(e).to_string())
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RESTError {
    #[error(transparent)]
    App(AppError),
    #[error("Internal server error: {0}")]
    InternalServerError(String),
    #[error("Missing field: {0}")]
    MissingField(String),
    #[error("Malformed field: {0}")]
    MalformedField(String),
    #[error("Duplicate field: {0}")]
    DuplicateField(String),
    #[error("Not Found: {0}")]
    NotFound(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Bad Request: {0}")]
    BadRequest(String),
}

// Anything that can be converted into an AppError can be converted into a RESTError
impl<T: Into<AppError>> From<T> for RESTError {
    fn from(e: T) -> Self {
        Self::App(e.into())
    }
}

impl IntoResponse for RESTError {
    fn into_response(self) -> Response {
        let self_str = self.to_string();

        let (status, error_message) = match self {
            Self::App(e) => return e.into_response(),
            Self::InternalServerError(message) => {
                tracing::error!(error = %message);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::MissingField(_) => (StatusCode::BAD_REQUEST, self_str),
            Self::MalformedField(_) => (StatusCode::BAD_REQUEST, self_str),
            Self::DuplicateField(_) => (StatusCode::BAD_REQUEST, self_str),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, self_str),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, self_str),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, self_str),
        };
        let body = Json(json!({
            "error": error_message
        }));
        (status, body).into_response()
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuthError {
    #[error("Wrong credentials")]
    WrongCredentials,
    #[error("Missing credentials")]
    MissingCredentials,
    #[error("User not found")]
    UserNotFound,
    #[error("Token creation failed")]
    TokenCreation,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Failed to generate password hash: {0}")]
    PasswordHash(#[from] argon2::password_hash::Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::WrongCredentials => StatusCode::UNAUTHORIZED,
            Self::MissingCredentials => StatusCode::UNAUTHORIZED,
            Self::UserNotFound => StatusCode::NOT_FOUND,
            Self::TokenCreation => StatusCode::INTERNAL_SERVER_ERROR,
            Self::InvalidToken => StatusCode::UNAUTHORIZED,
            Self::PasswordHash(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": self.to_string()
        }));
        (status, body).into_response()
    }
}
