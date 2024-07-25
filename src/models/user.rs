use std::{hash::Hash, sync::LazyLock};

use chrono::prelude::*;
use chrono::DateTime;
use derive_builder::Builder;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::gateway::handler::Gateway;

use super::{
    avatar::{Avatar, FullAvatar, PartialAvatar, UserAvatar},
    errors::BuildError,
    requests::{CreateUser, UpdateUser},
    snowflake::Snowflake,
    state::Config,
};

static USERNAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9]*(?:[._][a-zA-Z0-9]+)*[a-zA-Z0-9])$")
        .expect("Failed to compile username regex")
});

/// Represents the presence of a user.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(i16)]
pub enum Presence {
    /// The user is currently active.
    Online = 0,
    /// The user is idle or away from the keyboard.
    Away = 1,
    /// The user is busy. Clients should try to disable notifications in this state.
    Busy = 2,
    /// The user is offline or invisible.
    Offline = 3,
}

impl From<i16> for Presence {
    fn from(presence: i16) -> Self {
        match presence {
            0 => Self::Online,
            1 => Self::Away,
            2 => Self::Busy,
            _ => Self::Offline,
        }
    }
}

impl Default for Presence {
    fn default() -> Self {
        Self::Online
    }
}

/// Represents a user record stored in the database.
pub struct UserRecord {
    pub id: Snowflake<User>,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_hash: Option<String>,
    pub last_presence: i16,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Builder)]
#[builder(setter(into), build_fn(error = "BuildError"))]
pub struct User {
    /// The snowflake belonging to this user.
    id: Snowflake<User>,
    /// A user's username. This is unique to the user.
    username: String,
    /// A user's displayname.
    #[builder(default)]
    display_name: Option<String>,

    /// The user's avatar hash.
    #[serde(rename = "avatar_hash")]
    #[builder(default)]
    avatar: Option<Avatar<UserAvatar>>,

    /// The last presence used by this user.
    /// This does not represent the user's actual presence, as that also depends on the gateway connection.
    #[serde(skip)]
    #[builder(default)]
    last_presence: Presence,
    /// Is 'null' in all cases except when the user is sent in a `GUILD_CREATE` event.
    /// This is the presence that is sent in payloads to clients.
    #[serde(rename = "presence")]
    #[builder(setter(skip), default)]
    displayed_presence: Option<Presence>,
}

impl User {
    /// Create a new builder to construct a user.
    pub fn builder() -> UserBuilder {
        UserBuilder::default()
    }

    /// The snowflake belonging to this user.
    pub const fn id(&self) -> Snowflake<Self> {
        self.id
    }

    /// The user's creation date.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.id.created_at()
    }

    /// The user's username. This is unique to the user.
    pub const fn username(&self) -> &String {
        &self.username
    }

    /// The user's display name. This is the same as the username unless the user has changed it.
    pub const fn display_name(&self) -> Option<&String> {
        self.display_name.as_ref()
    }

    /// The user's display name. This is the same as the username unless the user has changed it.
    pub fn display_name_mut(&mut self) -> Option<&mut String> {
        self.display_name.as_mut()
    }

    /// The user's avatar.
    pub const fn avatar(&self) -> Option<&Avatar<UserAvatar>> {
        self.avatar.as_ref()
    }

    /// The last known presence of the user.
    ///
    /// This does not represent the user's actual presence, as that also depends on the gateway connection.
    pub const fn last_presence(&self) -> &Presence {
        &self.last_presence
    }

    /// Retrieve the user's presence.
    pub fn presence(&self, gateway: &Gateway) -> &Presence {
        if gateway.is_connected(self.id()) {
            &self.last_presence
        } else {
            &Presence::Offline
        }
    }

    /// Creates a new user object from a create user payload.
    ///
    /// ## Arguments
    ///
    /// * `config` - The application configuration.
    /// * `payload` - The payload to create the user from.
    ///
    /// ## Errors
    ///
    /// * [`BuilderError::ValidationError`] - If the username is invalid.
    pub fn from_payload(config: &Config, payload: &CreateUser) -> Result<Self, BuildError> {
        Self::validate_username(&payload.username)?;
        Ok(Self {
            id: Snowflake::gen_new(config),
            username: payload.username.clone(),
            display_name: None,
            avatar: None,
            last_presence: Presence::default(),
            displayed_presence: None,
        })
    }

    /// Build a user object directly from a database record.
    pub fn from_record(record: UserRecord) -> Self {
        Self {
            id: record.id,
            username: record.username,
            avatar: record.avatar_hash.map(|h| {
                Avatar::Partial(
                    PartialAvatar::<UserAvatar>::new(h, record.id).expect("Database should have valid avatar hash"),
                )
            }),
            display_name: record.display_name,
            last_presence: Presence::from(record.last_presence),
            displayed_presence: None,
        }
    }

    /// Update the model with new data.
    ///
    /// ## Arguments
    ///
    /// * `request` - The update request.
    ///
    /// ## Returns
    ///
    /// `true` if the user's avatar has changed, `false` otherwise.
    ///
    /// ## Errors
    ///
    /// * [`BuildError::ValidationError`] - If the new username is invalid.
    /// * [`BuildError::ValidationError`] - If the new avatar data is invalid.
    ///
    /// ## Note
    ///
    /// The avatar data still needs to be uploaded to S3.
    pub fn update(&mut self, request: UpdateUser) -> Result<bool, BuildError> {
        self.display_name = request.display_name;
        let mut has_avatar_changed = false;

        if let Some(username) = request.username {
            self.set_username(username)?;
        }

        if let Some(uri) = request.avatar {
            self.avatar = Some(Avatar::Full(FullAvatar::from_data_uri(&*self, uri)?));
            has_avatar_changed = true;
        }
        Ok(has_avatar_changed)
    }

    /// Transform this object to also include the user's presence.
    #[must_use]
    pub fn include_presence(self, gateway: &Gateway) -> Self {
        let presence = self.presence(gateway);
        Self {
            displayed_presence: Some(*presence),
            ..self
        }
    }

    /// Validates and sets a new username for this user.
    ///
    /// The username must be committed to the database for the change to take effect.
    ///
    /// ## Errors
    ///
    /// * [`BuilderError::ValidationError`] - If the username is invalid.
    pub fn set_username(&mut self, username: String) -> Result<(), BuildError> {
        Self::validate_username(&username)?;
        self.username = username;
        Ok(())
    }

    fn validate_username(username: &str) -> Result<&str, BuildError> {
        if !USERNAME_REGEX.is_match(username) {
            return Err(BuildError::ValidationError(format!(
                "Invalid username, must match regex: {}",
                USERNAME_REGEX.as_str()
            )));
        }
        if username.len() > 32 || username.len() < 3 {
            return Err(BuildError::ValidationError(
                "Invalid username, must be between 3 and 32 characters long".to_string(),
            ));
        }
        Ok(username)
    }
}

impl From<User> for Snowflake<User> {
    fn from(user: User) -> Self {
        user.id()
    }
}

impl From<&User> for Snowflake<User> {
    fn from(user: &User) -> Self {
        user.id()
    }
}

impl From<&mut User> for Snowflake<User> {
    fn from(user: &mut User) -> Self {
        user.id()
    }
}
