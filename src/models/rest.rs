use super::user::User;
use serde::{Deserialize, Serialize};

/// A request to create a new user
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateUser {
    pub username: String,
}

/// A response to a request to create a new user
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateUserResponse {
    /// The created user
    user: User,
    /// The login-token for the created user
    token: String,
}

impl CreateUserResponse {
    pub fn new(user: User, token: String) -> Self {
        CreateUserResponse { user, token }
    }
}

/// A request to create a new message
///
/// A Message object is returned on success
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateMessage {
    content: String,
}

impl CreateMessage {
    pub fn new(content: String) -> Self {
        CreateMessage { content }
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}
