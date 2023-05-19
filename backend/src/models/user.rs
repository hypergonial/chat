use super::appstate::APP;
use super::rest::CreateUser;
use chrono::prelude::*;
use chrono::DateTime;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::snowflake::Snowflake;

lazy_static! {
    static ref USERNAME_REGEX: Regex =
        Regex::new(r"^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9]*(?:[._][a-zA-Z0-9]+)*[a-zA-Z0-9])$")
            .unwrap();
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash)]
#[serde(tag = "presence", rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(i16)]
pub enum Presence {
    Online = 0,
    Away = 1,
    Busy = 2,
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

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub struct User {
    id: Snowflake,
    username: String,
    pub display_name: String,
    #[serde(skip)]
    last_presence: Presence,
}

impl User {
    pub fn new(id: Snowflake, username: String) -> Result<Self, anyhow::Error> {
        Self::validate_username(&username)?;
        Ok(User {
            id,
            username: username.clone(),
            display_name: username,
            last_presence: Presence::default(),
        })
    }

    pub async fn from_payload(payload: CreateUser) -> Result<Self, anyhow::Error> {
        Self::validate_username(&payload.username)?;
        Ok(User {
            id: Snowflake::gen_new().await,
            username: payload.username.clone(),
            display_name: payload.username,
            last_presence: Presence::default(),
        })
    }

    pub fn id(&self) -> Snowflake {
        self.id
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.id.created_at()
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub async fn presence(&self) -> &Presence {
        if APP.read().await.gateway.is_connected(self.id()) {
            &self.last_presence
        } else {
            &Presence::Offline
        }
    }

    pub fn set_username(&mut self, username: String) -> Result<(), anyhow::Error> {
        Self::validate_username(&username)?;
        self.username = username;
        Ok(())
    }

    fn validate_username(username: &str) -> Result<(), anyhow::Error> {
        if !USERNAME_REGEX.is_match(username) {
            anyhow::bail!(
                "Invalid username, must match regex: {}",
                USERNAME_REGEX.to_string()
            );
        }
        if username.len() > 32 || username.len() < 3 {
            anyhow::bail!("Invalid username, must be between 3 and 32 characters long");
        }
        Ok(())
    }

    /// Retrieve a user from the database by their ID.
    pub async fn fetch(id: Snowflake) -> Option<Self> {
        let db = &APP.read().await.db;
        let id_i64: i64 = id.into();
        let row = sqlx::query!(
            "SELECT username, display_name, last_presence
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;

        Some(User {
            id,
            username: row.username,
            display_name: row.display_name,
            last_presence: Presence::from(row.last_presence),
        })
    }

    /// Retrieve a user from the database by their username.
    pub async fn fetch_by_username(username: &str) -> Option<Self> {
        let db = &APP.read().await.db;
        let row = sqlx::query!(
            "SELECT id, username, display_name, last_presence
            FROM users
            WHERE username = $1",
            username
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;

        Some(User {
            id: row.id.into(),
            username: row.username,
            display_name: row.display_name,
            last_presence: Presence::from(row.last_presence),
        })
    }

    /// Commit this user to the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = &APP.read().await.db;
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
        .execute(db.pool())
        .await?;
        Ok(())
    }
}
