use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{appstate::APP, member::UserLike, rest::CreateMessage, snowflake::Snowflake, user::User};

/// A chat message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// The id of the message.
    id: Snowflake,
    /// The id of the channel this message was sent in.
    channel_id: Snowflake,
    /// The author of the message.
    author: Option<UserLike>,
    /// A nonce that can be used by a client to determine if the message was sent.
    /// The nonce is not stored in the database and thus is not returned by REST calls.
    nonce: Option<String>,
    /// The content of the message.
    pub content: String,
}

impl Message {
    /// Create a new message with the given id, author, content, and nonce.
    pub fn new(id: Snowflake, channel_id: Snowflake, author: UserLike, content: String, nonce: Option<String>) -> Self {
        Message {
            id,
            channel_id,
            author: Some(author),
            content,
            nonce,
        }
    }

    /// Create a new message from the given payload. Assigns a new snowflake to the message.
    pub async fn from_payload(author: UserLike, channel_id: Snowflake, payload: CreateMessage) -> Self {
        Message {
            id: Snowflake::gen_new().await,
            channel_id,
            author: Some(author),
            content: payload.content().to_string(),
            nonce: payload.nonce().clone(),
        }
    }

    /// The unique ID of this message.
    pub fn id(&self) -> Snowflake {
        self.id
    }

    /// The user who sent this message.
    ///
    /// This may be `None` if the author has been deleted since.
    pub fn author(&self) -> &Option<UserLike> {
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
            "SELECT user_id, channel_id, content
            FROM messages
            WHERE id = $1",
            id_i64
        )
        .fetch_optional(db.pool())
        .await
        .ok()??;

        let mut author = None;
        if row.user_id.is_some() {
            author = Some(UserLike::User(User::fetch(row.user_id.unwrap().into()).await?));
        }

        Some(Self {
            id,
            channel_id: row.channel_id.into(),
            author,
            content: row.content,
            nonce: None,
        })
    }

    /// Commit this message to the database.
    pub async fn commit(&self) -> Result<(), sqlx::Error> {
        let db = &APP.read().await.db;
        let id_i64: i64 = self.id.into();
        let author_id_i64: Option<i64> = self.author.as_ref().map(|u| u.id().into());
        let channel_id_i64: i64 = self.channel_id.into();
        sqlx::query!(
            "INSERT INTO messages (id, user_id, channel_id, content)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET user_id = $2, channel_id = $3, content = $4",
            id_i64,
            author_id_i64,
            channel_id_i64,
            self.content
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }
}
