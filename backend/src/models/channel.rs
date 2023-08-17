use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use sqlx::Error as SqlxError;

use super::{appstate::APP, errors::AppError, message::ExtendedMessageRecord, rest::CreateChannel};
use super::{message::Message, snowflake::Snowflake};

#[async_trait]
#[enum_dispatch(Channel)]
pub trait ChannelLike {
    /// The Snowflake ID of a channel.
    fn id(&self) -> Snowflake;
    /// The Snowflake ID of the guild this channel belongs to.
    fn guild_id(&self) -> Snowflake;
    /// The name of the channel.
    fn name(&self) -> &str;
    /// The name of the channel.
    fn name_mut(&mut self) -> &mut String;
    /// Commit this channel's current state to the database.
    async fn commit(&self) -> Result<(), SqlxError>;
    /// Deletes the channel.
    async fn delete(self) -> Result<(), AppError>;
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
        let db = APP.db.read().await;
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
                Self::GuildText(TextChannel::new(Snowflake::gen_new(), guild_id, name))
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
    pub fn new(id: Snowflake, guild: impl Into<Snowflake>, name: String) -> Self {
        Self {
            id,
            guild_id: guild.into(),
            name,
        }
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

        let db = APP.db.read().await;
        let id_64: i64 = self.id.into();

        let records: Vec<ExtendedMessageRecord> = if before.is_none() && after.is_none() {
            // SAFETY: Must use `query_as_unchecked` because `ExtendedMessageRecord`
            // contains `Option<T>` for all users fields and sqlx does not recognize this.
            sqlx::query_as_unchecked!(
                ExtendedMessageRecord,
                "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
                FROM messages
                LEFT JOIN users ON messages.user_id = users.id
                LEFT JOIN attachments ON messages.id = attachments.message_id
                WHERE messages.channel_id = $1
                ORDER BY messages.id DESC LIMIT $2",
                id_64,
                limit as i64
            )
            .fetch_all(db.pool())
            .await?
        } else {
            // SAFETY: Ditto, see above.
            sqlx::query_as_unchecked!(
                ExtendedMessageRecord,
                "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
                FROM messages
                LEFT JOIN users ON messages.user_id = users.id
                LEFT JOIN attachments ON messages.id = attachments.message_id
                WHERE messages.channel_id = $1 AND messages.id > $2 AND messages.id < $3
                ORDER BY messages.id DESC LIMIT $4",
                id_64,
                before.map(|s| s.into()).unwrap_or(i64::MAX),
                after.map(|s| s.into()).unwrap_or(i64::MIN),
                limit as i64
            )
            .fetch_all(db.pool())
            .await?
        };
        Ok(Message::from_records(&records))
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
        let db = APP.db.read().await;
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
    async fn delete(self) -> Result<(), AppError> {
        let db = APP.db.read().await;
        let id_64: i64 = self.id.into();

        APP.buckets().remove_all_for_channel(self).await?;

        sqlx::query!("DELETE FROM channels WHERE id = $1", id_64)
            .execute(db.pool())
            .await?;

        Ok(())
    }
}

impl From<Channel> for Snowflake {
    fn from(channel: Channel) -> Self {
        channel.id()
    }
}

impl From<TextChannel> for Snowflake {
    fn from(channel: TextChannel) -> Self {
        channel.id()
    }
}

impl From<&Channel> for Snowflake {
    fn from(channel: &Channel) -> Self {
        channel.id()
    }
}

impl From<&TextChannel> for Snowflake {
    fn from(channel: &TextChannel) -> Self {
        channel.id()
    }
}
