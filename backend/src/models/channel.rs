use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use futures_util::stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::Error as SqlxError;

use crate::models::message::MessageRecord;

use super::{appstate::APP, rest::CreateChannel};
use super::{message::Message, snowflake::Snowflake};

#[async_trait]
#[enum_dispatch(Channel)]
pub trait ChannelLike {
    fn id(&self) -> Snowflake;
    fn guild_id(&self) -> Snowflake;
    fn name(&self) -> &str;
    fn name_mut(&mut self) -> &mut String;
    async fn commit(&self) -> Result<(), SqlxError>;
    async fn delete(self) -> Result<(), SqlxError>;
}

/// Represents a row representing a channel.
pub struct ChannelRecord {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub channel_type: String,
}

#[non_exhaustive]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[enum_dispatch]
pub enum Channel {
    GuildText(TextChannel),
}

impl Channel {
    pub fn from_record(record: ChannelRecord) -> Self {
        match record.channel_type.as_str() {
            "TEXT_CHANNEL" => Self::GuildText(TextChannel::new(
                Snowflake::from(record.id),
                Snowflake::from(record.guild_id),
                record.name,
            )),
            _ => panic!("Invalid channel type"),
        }
    }

    pub async fn fetch(id: Snowflake) -> Option<Self> {
        let db = &APP.db.read().await;
        let id_64: i64 = id.into();

        let record = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE id = $1", id_64)
            .fetch_optional(db.pool())
            .await
            .ok()??;

        Some(Self::from_record(record))
    }

    pub async fn from_payload(payload: CreateChannel, guild_id: Snowflake) -> Self {
        match payload {
            CreateChannel::GuildText { name } => {
                Self::GuildText(TextChannel::new(Snowflake::gen_new().await, guild_id, name))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TextChannel {
    id: Snowflake,
    guild_id: Snowflake,
    name: String,
}

impl TextChannel {
    pub fn new(id: Snowflake, guild_id: Snowflake, name: String) -> Self {
        Self { id, guild_id, name }
    }

    /// Fetch messages from this channel.
    /// 
    /// ## Arguments
    /// 
    /// * `limit` - The maximum number of messages to fetch. Defaults to 50, capped at 100.
    /// * `before` - Fetch messages before this ID.
    /// * `after` - Fetch messages after this ID.
    /// 
    /// ## Returns
    /// 
    /// [`Vec<Message>`] - The messages fetched.
    /// 
    /// ## Locks
    /// 
    /// * `APP.db` (read)
    /// 
    /// ## Errors
    /// 
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_messages(
        &self,
        limit: Option<u32>,
        before: Option<Snowflake>,
        after: Option<Snowflake>,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        let db = &APP.db.read().await;
        let id_64: i64 = self.id.into();

        let records: Vec<MessageRecord>;

        if before.is_none() && after.is_none() {
            records = sqlx::query_as!(
                MessageRecord,
                "SELECT * FROM messages WHERE channel_id = $1 ORDER BY id DESC LIMIT $2",
                id_64,
                limit as i64
            )
            .fetch_all(db.pool())
            .await?;
        } else {
            records = sqlx::query_as!(
                MessageRecord,
                "SELECT * FROM messages WHERE channel_id = $1 AND id > $2 AND id < $3 ORDER BY id DESC LIMIT $4",
                id_64,
                before.map(|s| s.into()).unwrap_or(i64::MAX),
                after.map(|s| s.into()).unwrap_or(i64::MIN),
                limit as i64
            )
            .fetch_all(db.pool())
            .await?;
        }

        Ok(stream::iter(records)
            .then(|r| async move { Message::from_record(&r).await })
            .collect()
            .await)
    }
}

#[async_trait]
impl ChannelLike for TextChannel {
    fn id(&self) -> Snowflake {
        self.id
    }

    fn guild_id(&self) -> Snowflake {
        self.guild_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    /// Commit this channel to the database.
    /// 
    /// ## Locks
    /// 
    /// * `APP.db` (read)
    /// 
    /// ## Errors
    /// 
    /// * [`sqlx::Error`] - If the database query fails.
    async fn commit(&self) -> Result<(), SqlxError> {
        let db = &APP.db.read().await;
        let id_64: i64 = self.id.into();
        let guild_id_64: i64 = self.guild_id.into();
        sqlx::query!(
            "INSERT INTO channels (id, guild_id, name, channel_type)
            VALUES ($1, $2, $3, 'TEXT_CHANNEL')
            ON CONFLICT (id) DO UPDATE
            SET name = $3",
            id_64,
            guild_id_64,
            self.name
        )
        .execute(db.pool())
        .await?;

        Ok(())
    }

    /// Deletes the channel.
    async fn delete(self) -> Result<(), SqlxError> {
        let db = &APP.db.read().await;
        let id_64: i64 = self.id.into();
        sqlx::query!("DELETE FROM channels WHERE id = $1", id_64)
            .execute(db.pool())
            .await?;

        Ok(())
    }
}
