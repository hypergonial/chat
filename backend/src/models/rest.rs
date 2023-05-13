use secrecy::Secret;
use serde::{Deserialize, Serialize};

/// A request to create a new user
#[derive(Deserialize, Debug, Clone)]
pub struct CreateUser {
    pub username: String,
    pub password: Secret<String>,
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
