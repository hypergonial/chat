use secrecy::Secret;
use serde::Deserialize;

use super::{
    channel::Channel,
    data_uri::DataUri,
    errors::AppError,
    guild::Guild,
    member::Member,
    prefs::{Layout, PrefFlags},
    snowflake::Snowflake,
    state::ApplicationState,
    user::User,
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

impl CreateGuild {
    /// Perform the create operation
    ///
    /// This is a shorthand for `app.ops().create_guild(payload, owner).await`
    ///
    /// # Parameters
    ///
    /// - `app` - The application state
    /// - `owner` - The owner of the guild
    ///
    /// # Returns
    ///
    /// The created guild, channel, and member
    ///
    /// # Errors
    ///
    /// Fails if the guild creation fails
    #[inline]
    pub async fn perform_request(
        self,
        app: &ApplicationState,
        owner: impl Into<Snowflake<User>>,
    ) -> Result<(Guild, Channel, Member), sqlx::Error> {
        app.ops().create_guild(self, owner).await
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct UpdateGuild {
    pub name: Option<String>,
    pub owner_id: Option<Snowflake<User>>,
    pub avatar: Option<DataUri>,
}

impl UpdateGuild {
    /// Perform the update operation
    ///
    /// This is a shorthand for `app.ops().update_guild(payload).await`
    ///
    /// # Parameters
    ///
    /// - `app` - The application state
    /// - `guild` - The current guild state that needs to be updated
    ///
    /// # Returns
    ///
    /// The updated guild
    ///
    /// # Errors
    ///
    /// Fails if the guild does not exist or the update operation fails
    #[inline]
    pub async fn perform_request(self, app: &ApplicationState, guild: &Guild) -> Result<Guild, AppError> {
        app.ops().update_guild(self, guild).await
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CreateChannel {
    GuildText { name: String },
}

#[derive(Deserialize, Debug, Clone)]
pub struct UpdateUser {
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar: Option<DataUri>,
}

impl UpdateUser {
    /// Perform the update operation
    /// This is a shorthand for `app.ops().update_user(user, payload).await`
    ///
    /// # Parameters
    ///
    /// - `app` - The application state
    /// - `user` - The user to update
    ///
    /// # Returns
    ///
    /// The updated user
    ///
    /// # Errors
    ///
    /// Fails if the user does not exist or the update operation fails
    #[inline]
    pub async fn perform_request(
        self,
        app: &ApplicationState,
        user: impl Into<Snowflake<User>>,
    ) -> Result<User, AppError> {
        app.ops().update_user(user, self).await
    }
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
