use super::db::DB;
use chrono::prelude::*;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

use super::snowflake::Snowflake;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    id: Snowflake,
    pub username: String,
}

impl User {
    pub fn new(id: Snowflake, username: String) -> Self {
        User { id, username }
    }

    pub fn id(&self) -> Snowflake {
        self.id
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.id.created_at()
    }

    /// Retrieve a user from the database by their ID.
    pub async fn fetch(id: Snowflake) -> Option<Self> {
        let db = DB.read().await;
        let id_i64: i64 = id.into();
        let row = sqlx::query!(
            "SELECT username
            FROM users
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(db.pool())
        .await
        .ok()?;
        Some(User::new(id, row?.username))
    }

    /// Commit this user to the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = DB.read().await;
        let id_i64: i64 = self.id.into();
        sqlx::query!(
            "INSERT INTO users (id, username)
            VALUES ($1, $2)
            ON CONFLICT (id) DO UPDATE
            SET username = $2",
            id_i64,
            self.username
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }
}

impl Hash for User {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
