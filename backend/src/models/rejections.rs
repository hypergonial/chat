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

/// A rejection triggered when an invalid token or credentials are provided.
#[derive(Debug)]
pub struct Unauthorized {
    pub message: String,
    pub header: String,
}

impl Unauthorized {
    /// Create a new Unauthorized rejection with a custom message and header parameters.
    /// The scheme and realm parameters are used to set the `WWW-Authenticate` header.
    ///
    /// ## Arguments
    ///
    /// * `message` - The message to return in the response body
    /// * `scheme` - The authentication scheme to use in the `WWW-Authenticate` header
    /// * `realm` - The realm to use in the `WWW-Authenticate` header
    pub fn new(message: &str, scheme: &str, realm: &str) -> Self {
        Unauthorized {
            message: message.to_string(),
            header: format!("{} realm=\"{}\"", scheme, realm),
        }
    }

    /// Create a new Unauthorized rejection with a custom message and the `Bearer` scheme.
    pub fn bearer(realm: &str) -> Self {
        Unauthorized::new("Invalid token provided.", "Bearer", realm)
    }

    /// Create a new Unauthorized rejection with a custom message and the `Basic` scheme.
    pub fn basic(realm: &str) -> Self {
        Unauthorized::new("Invalid credentials provided.", "Basic", realm)
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

    /// Create a new InternalServerError with a default database error message
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

/// A rejection triggered when a request is rate limited.
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

/// A rejection triggered when a request is forbidden.
/// This could be due to missing priviliges or a visibility restriction.
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
