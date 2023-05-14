use secrecy::Secret;
use serde::{Deserialize, Serialize};

use super::snowflake::Snowflake;

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
    nonce: Option<String>,
}

impl CreateMessage {
    pub fn new(content: String, nonce: Option<String>) -> Self {
        CreateMessage { content, nonce }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn nonce(&self) -> &Option<String> {
        &self.nonce
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct AuthUserResponse {
    user_id: Snowflake,
    token: String,
}

impl AuthUserResponse {
    pub fn new(user_id: Snowflake, token: String) -> Self {
        AuthUserResponse { user_id, token }
    }

    pub fn user_id(&self) -> Snowflake {
        self.user_id
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}
