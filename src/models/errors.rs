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

/// Errors encountered during object initialization.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum BuildError {
    /// A field was not initialized before calling `.build()` in a builder.
    #[error("Uninitialized field: {0}")]
    UninitializedField(&'static str),
    /// A validation check failed.
    #[error("Validation error: {0}")]
    ValidationError(String),
}

impl From<UninitializedFieldError> for BuildError {
    fn from(e: UninitializedFieldError) -> Self {
        Self::UninitializedField(e.field_name())
    }
}

impl From<String> for BuildError {
    fn from(e: String) -> Self {
        Self::ValidationError(e)
    }
}

impl IntoResponse for BuildError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::UninitializedField(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
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
    Build(#[from] BuildError),
    #[error("Failed to parse int: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("Authentication failure: {0}")]
    Auth(#[from] AuthError),
    #[error("Internal Server Error: {0}")]
    Axum(#[from] axum::Error),
    #[error("Not Found: {0}")]
    NotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Multipart(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Regex(_) | Self::ParseInt(_) | Self::JWT(_) | Self::JSON(_) => StatusCode::BAD_REQUEST,
            Self::Build(e) => return e.into_response(),
            Self::Axum(_) | Self::Database(_) | Self::S3(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Auth(e) => return e.into_response(),
            Self::NotFound(_) => StatusCode::NOT_FOUND,
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self);
        }
        let body = Json(json!({
            "error": self.to_string()
        }));
        (status, body).into_response()
    }
}

/// Hacky workaround for `SdkError` having a generic type parameter
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
pub enum GatewayError {
    #[error(transparent)]
    App(AppError),
    #[error("Internal server error: {0}")]
    InternalServerError(String),
    #[error("Policy Violation: {0}")]
    PolicyViolation(String),
    #[error("Malformed frame: {0}")]
    MalformedFrame(String),
    #[error("Auth error: {0}")]
    AuthError(String),
    #[error("Handshake failure: {0}")]
    HandshakeFailure(String),
}

// Anything that can be converted into an AppError can be converted into a RESTError
impl<T: Into<AppError>> From<T> for GatewayError {
    fn from(e: T) -> Self {
        Self::App(e.into())
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
            Self::MissingField(_) | Self::MalformedField(_) | Self::DuplicateField(_) => {
                (StatusCode::BAD_REQUEST, self_str)
            }
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
    /// Sent when the user provides invalid username/password.
    #[error("Invalid credentials")]
    InvalidCredentials,
    /// Sent when authorization is required but no token was provided.
    #[error("Missing or malformed credentials")]
    MissingCredentials,
    /// Sent when the server fails to create a token.
    #[error("Token creation failed")]
    TokenCreation,
    /// Sent when the user provides an invalid token.
    #[error("Invalid token")]
    InvalidToken,
    /// Sent when the server fails to hash a password.
    #[error("Failed to generate password hash: {0}")]
    PasswordHash(#[from] argon2::password_hash::Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::MissingCredentials | Self::TokenCreation | Self::InvalidToken | Self::InvalidCredentials => {
                StatusCode::UNAUTHORIZED
            }
            Self::PasswordHash(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": self.to_string()
        }));
        (status, body).into_response()
    }
}
