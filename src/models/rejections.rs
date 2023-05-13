use serde::Serialize;
use warp::reject;

/// JSON-serializable API error
#[derive(Serialize)]
pub struct ErrorMessage {
    code: u16,
    message: String,
    description: Option<String>,
}

impl ErrorMessage {
    pub fn new(code: u16, message: String, description: Option<String>) -> Self {
        ErrorMessage {
            code,
            message,
            description,
        }
    }
}

/// A rejection triggered when an invalid token is provided
#[derive(Debug)]
pub struct Unauthorized;

impl reject::Reject for Unauthorized {}

/// A rejection triggered when the author and token-holder don't match
#[derive(Debug)]
pub struct AuthorMismatch;

impl reject::Reject for AuthorMismatch {}

/// A rejection triggered for any other error
#[derive(Debug)]
pub struct InternalServerError;

impl reject::Reject for InternalServerError {}
