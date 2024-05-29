use std::sync::OnceLock;

use chrono::prelude::*;
use chrono::DateTime;
use derive_builder::Builder;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::gateway::handler::Gateway;

use super::{appstate::Config, errors::BuilderError, requests::CreateUser, snowflake::Snowflake};

fn username_regex() -> &'static Regex {
    static USERNAME_REGEX: OnceLock<Regex> = OnceLock::new();
    USERNAME_REGEX.get_or_init(|| {
        Regex::new(r"^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9]*(?:[._][a-zA-Z0-9]+)*[a-zA-Z0-9])$")
            .expect("Failed to compile username regex")
    })
}

/// Represents the presence of a user.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash)]
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
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub last_presence: i16,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone, Builder)]
#[builder(setter(into), build_fn(error = "BuilderError"))]
pub struct User {
    /// The snowflake belonging to this user.
    id: Snowflake<User>,
    /// A user's username. This is unique to the user.
    username: String,
    /// A user's displayname.
    #[builder(default)]
    pub display_name: Option<String>,
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
    pub fn from_payload(config: &Config, payload: &CreateUser) -> Result<Self, BuilderError> {
        Self::validate_username(&payload.username)?;
        Ok(Self {
            id: Snowflake::gen_new(config),
            username: payload.username.clone(),
            display_name: None,
            last_presence: Presence::default(),
            displayed_presence: None,
        })
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

    /// Build a user object directly from a database record.
    pub fn from_record(record: UserRecord) -> Self {
        Self {
            id: Snowflake::from(record.id),
            username: record.username,
            display_name: record.display_name,
            last_presence: Presence::from(record.last_presence),
            displayed_presence: None,
        }
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
    pub fn set_username(&mut self, username: String) -> Result<(), BuilderError> {
        Self::validate_username(&username)?;
        self.username = username;
        Ok(())
    }

    fn validate_username(username: &str) -> Result<(), BuilderError> {
        if !username_regex().is_match(username) {
            return Err(BuilderError::ValidationError(format!(
                "Invalid username, must match regex: {}",
                username_regex().as_str()
            )));
        }
        if username.len() > 32 || username.len() < 3 {
            return Err(BuilderError::ValidationError(
                "Invalid username, must be between 3 and 32 characters long".to_string(),
            ));
        }
        Ok(())
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
