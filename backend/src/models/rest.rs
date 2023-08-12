use secrecy::Secret;
use serde::Deserialize;

/// A request to create a new user
#[derive(Deserialize, Debug, Clone)]
pub struct CreateUser {
    pub username: String,
    pub password: Secret<String>,
}

/// The JSON part of a multipart form request to create a message
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMessage {
    pub content: Option<String>,
    pub nonce: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CreateGuild {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CreateChannel {
    GuildText { name: String },
}
