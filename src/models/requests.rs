use secrecy::Secret;
use serde::Deserialize;

use super::{
    data_uri::DataUri,
    prefs::{Layout, PrefFlags},
};

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

#[derive(Deserialize, Debug, Clone)]
pub struct UpdateUser {
    pub display_name: Option<String>,
    pub avatar: Option<DataUri>,
}

/// Update payload for user preferences
#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePrefs {
    pub flags: Option<PrefFlags>,
    pub message_grouping_timeout: Option<u64>,
    pub layout: Option<Layout>,
    pub text_size: Option<u8>,
    pub locale: Option<String>,
}
