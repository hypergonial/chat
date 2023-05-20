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

#[derive(Debug)]
pub struct NotFound {
    pub message: String,
}

impl NotFound {
    pub fn new(message: &str) -> Self {
        NotFound {
            message: message.to_string(),
        }
    }
}

impl reject::Reject for NotFound {}

/// A rejection triggered when an invalid token is provided
#[derive(Debug)]
pub struct Unauthorized {
    pub message: String,
}

impl Unauthorized {
    pub fn new(message: &str) -> Self {
        Unauthorized {
            message: message.to_string(),
        }
    }
}

impl reject::Reject for Unauthorized {}

/// A rejection triggered for any other error
#[derive(Debug)]
pub struct InternalServerError {
    pub message: String,
}

impl InternalServerError {
    pub fn new(message: &str) -> Self {
        InternalServerError {
            message: message.to_string(),
        }
    }

    pub fn db() -> Self {
        InternalServerError {
            message: "A database transaction error occured.".to_string(),
        }
    }
}

impl reject::Reject for InternalServerError {}

#[derive(Debug)]
pub struct BadRequest {
    pub message: String,
}

impl BadRequest {
    pub fn new(message: &str) -> Self {
        BadRequest {
            message: message.to_string(),
        }
    }
}

impl reject::Reject for BadRequest {}

#[derive(Debug)]
pub struct RateLimited {
    pub message: String,
}

impl RateLimited {
    pub fn new(message: &str) -> Self {
        RateLimited {
            message: message.to_string(),
        }
    }
}

impl reject::Reject for RateLimited {}

#[derive(Debug)]
pub struct Forbidden {
    pub message: String,
}

impl Forbidden {
    pub fn new(message: &str) -> Self {
        Forbidden {
            message: message.to_string(),
        }
    }
}

impl reject::Reject for Forbidden {}
