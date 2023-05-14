use super::{appstate::APP, snowflake::Snowflake, user::User};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A chat message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// The id of the message.
    id: Snowflake,
    /// The author of the message.
    author: User,
    /// A nonce that can be used by a client to determine if the message was sent.
    /// The nonce is not stored in the database and thus is not returned by REST calls.
    nonce: Option<String>,
    /// The content of the message.
    pub content: String,
}

impl Message {
    pub fn new(id: Snowflake, author: User, content: String, nonce: Option<String>) -> Self {
        Message {
            id,
            author,
            content,
            nonce,
        }
    }

    /// The unique ID of this message.
    pub fn id(&self) -> Snowflake {
        self.id
    }

    /// The user who sent this message.
    pub fn author(&self) -> &User {
        &self.author
    }

    /// The time at which this message was sent.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.id.created_at()
    }

    /// Retrieve a message from the database by its ID.
    pub async fn fetch(id: Snowflake) -> Option<Self> {
        let db = &APP.read().await.db;
        let id_i64: i64 = id.into();
        let row = sqlx::query!(
            "SELECT user_id, content
            FROM messages
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;
        let author = User::fetch(row.user_id.into()).await?;
        Some(Message::new(id, author, row.content, None))
    }

    /// Commit this message to the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = &APP.read().await.db;
        let id_i64: i64 = self.id.into();
        let author_id_i64: i64 = self.author.id().into();
        sqlx::query!(
            "INSERT INTO messages (id, user_id, content)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET user_id = $2, content = $3",
            id_i64,
            author_id_i64,
            self.content
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }
}
