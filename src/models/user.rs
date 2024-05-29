use std::sync::OnceLock;

use chrono::prelude::*;
use chrono::DateTime;
use derive_builder::Builder;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::models::guild::GuildRecord;

use super::{appstate::SharedState, errors::BuilderError, guild::Guild, requests::CreateUser, snowflake::Snowflake};

fn username_regex() -> &'static Regex {
    static USERNAME_REGEX: OnceLock<Regex> = OnceLock::new();
    USERNAME_REGEX
        .get_or_init(|| Regex::new(r"^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9]*(?:[._][a-zA-Z0-9]+)*[a-zA-Z0-9])$").unwrap())
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
        if !(0..=3).contains(&presence) {
            Self::Offline
        } else {
            // SAFETY: We checked bounds
            unsafe { std::mem::transmute(presence) }
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
    /// Is 'null' in all cases except when the user is sent in a GUILD_CREATE event.
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
    pub async fn from_payload(app: SharedState, payload: CreateUser) -> Result<Self, BuilderError> {
        Self::validate_username(&payload.username)?;
        Ok(User {
            id: Snowflake::gen_new(app),
            username: payload.username.clone(),
            display_name: None,
            last_presence: Presence::default(),
            displayed_presence: None,
        })
    }

    /// The snowflake belonging to this user.
    pub fn id(&self) -> Snowflake<User> {
        self.id
    }

    /// The user's creation date.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.id.created_at()
    }

    /// The user's username. This is unique to the user.
    pub fn username(&self) -> &String {
        &self.username
    }

    /// The user's display name. This is the same as the username unless the user has changed it.
    pub fn display_name(&self) -> &Option<String> {
        &self.display_name
    }

    /// The last known presence of the user.
    ///
    /// This does not represent the user's actual presence, as that also depends on the gateway connection.
    pub fn last_presence(&self) -> &Presence {
        &self.last_presence
    }

    /// Retrieve the user's presence.
    pub async fn presence(&self, app: SharedState) -> &Presence {
        if app.gateway.is_connected(self.id()) {
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
    pub async fn include_presence(self, app: SharedState) -> Self {
        let presence = self.presence(app).await;
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

    /// Retrieve a user from the database by their ID.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    pub async fn fetch(app: SharedState, id: impl Into<Snowflake<User>>) -> Option<Self> {
        let id_i64: i64 = id.into().into();
        let row = sqlx::query_as!(
            UserRecord,
            "SELECT id, username, display_name, last_presence
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(app.db.pool())
        .await
        .ok()??;

        Some(Self::from_record(row))
    }

    /// Fetch the presence of a user.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    pub async fn fetch_presence(app: SharedState, user: impl Into<Snowflake<User>>) -> Option<Presence> {
        let id_i64: i64 = user.into().into();
        let row = sqlx::query!(
            "SELECT last_presence
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(app.db.pool())
        .await
        .ok()??;

        Some(Presence::from(row.last_presence))
    }

    /// Retrieve a user from the database by their username.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    pub async fn fetch_by_username(app: SharedState, username: &str) -> Option<Self> {
        let row = sqlx::query!(
            "SELECT id, username, display_name, last_presence
            FROM users
            WHERE username = $1",
            username
        )
        .fetch_optional(app.db.pool())
        .await
        .ok()??;

        Some(User {
            id: row.id.into(),
            username: row.username,
            display_name: row.display_name,
            last_presence: Presence::from(row.last_presence),
            displayed_presence: None,
        })
    }

    /// Fetch all guilds that this user is a member of.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_guilds(&self, app: SharedState) -> Result<Vec<Guild>, sqlx::Error> {
        let id_i64: i64 = self.id.into();

        let records = sqlx::query_as!(
            GuildRecord,
            "SELECT guilds.id, guilds.name, guilds.owner_id
            FROM guilds
            INNER JOIN members ON members.guild_id = guilds.id
            WHERE members.user_id = $1",
            id_i64
        )
        .fetch_all(app.db.pool())
        .await?;

        Ok(records.into_iter().map(Guild::from_record).collect())
    }

    /// Commit this user to the database.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn commit(&self, app: SharedState) -> Result<(), sqlx::Error> {
        let id_i64: i64 = self.id.into();
        sqlx::query!(
            "INSERT INTO users (id, username, display_name, last_presence)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET username = $2, display_name = $3, last_presence = $4",
            id_i64,
            self.username,
            self.display_name,
            self.last_presence as i16
        )
        .execute(app.db.pool())
        .await?;
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
