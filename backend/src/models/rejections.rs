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
pub struct Unauthorized {
    pub message: String,
}

impl reject::Reject for Unauthorized {}

/// A rejection triggered for any other error
#[derive(Debug)]
pub struct InternalServerError {
    pub message: String,
}

impl reject::Reject for InternalServerError {}

#[derive(Debug)]
pub struct BadRequest {
    pub message: String,
}

impl reject::Reject for BadRequest {}

#[derive(Debug)]
pub struct RateLimited {
    pub message: String,
}

impl reject::Reject for RateLimited {}

#[derive(Debug)]
pub struct Forbidden {
    pub message: String,
}

impl reject::Reject for Forbidden {}
