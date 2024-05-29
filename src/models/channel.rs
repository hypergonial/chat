use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use sqlx::Error as SqlxError;

use super::{
    appstate::SharedState, errors::AppError, guild::Guild, message::ExtendedMessageRecord, requests::CreateChannel,
};
use super::{message::Message, snowflake::Snowflake};

#[enum_dispatch(Channel)]
pub trait ChannelLike {
    /// The Snowflake ID of a channel.
    fn id(&self) -> Snowflake<Channel>;
    /// The Snowflake ID of the guild this channel belongs to.
    fn guild_id(&self) -> Snowflake<Guild>;
    /// The name of the channel.
    fn name(&self) -> &str;
    /// The name of the channel.
    fn name_mut(&mut self) -> &mut String;
    /// Commit this channel's current state to the database.
    async fn commit(&self, app: SharedState) -> Result<(), SqlxError>;
    /// Deletes the channel.
    async fn delete(&mut self, app: SharedState) -> Result<(), AppError>;
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

    pub async fn fetch(app: SharedState, id: Snowflake<Channel>) -> Option<Self> {
        let id_64: i64 = id.into();

        let record = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE id = $1", id_64)
            .fetch_optional(app.db.pool())
            .await
            .ok()??;

        Some(Self::from_record(record))
    }

    pub async fn from_payload(app: SharedState, payload: CreateChannel, guild_id: Snowflake<Guild>) -> Self {
        match payload {
            CreateChannel::GuildText { name } => {
                Self::GuildText(TextChannel::new(Snowflake::gen_new(app), guild_id, name))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TextChannel {
    id: Snowflake<Channel>,
    guild_id: Snowflake<Guild>,
    name: String,
}

impl TextChannel {
    pub fn new(id: Snowflake<Channel>, guild: impl Into<Snowflake<Guild>>, name: String) -> Self {
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
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn fetch_messages(
        &self,
        app: SharedState,
        limit: Option<u32>,
        before: Option<Snowflake<Message>>,
        after: Option<Snowflake<Message>>,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

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
            .fetch_all(app.db.pool())
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
            .fetch_all(app.db.pool())
            .await?
        };
        Ok(Message::from_records(&records))
    }
}

impl ChannelLike for TextChannel {
    fn id(&self) -> Snowflake<Channel> {
        self.id
    }

    fn guild_id(&self) -> Snowflake<Guild> {
        self.guild_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    /// Commit this channel to the database.
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    async fn commit(&self, app: SharedState) -> Result<(), SqlxError> {
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
        .execute(app.db.pool())
        .await?;

        Ok(())
    }

    /// Deletes the channel.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to delete all attachments fails.
    /// * [`AppError::Database`] - If the database query fails.
    async fn delete(&mut self, app: SharedState) -> Result<(), AppError> {
        let id_64: i64 = self.id.into();

        app.buckets.remove_all_for_channel(&app.s3, self.id()).await?;

        sqlx::query!("DELETE FROM channels WHERE id = $1", id_64)
            .execute(app.db.pool())
            .await?;

        Ok(())
    }
}

impl From<Channel> for Snowflake<Channel> {
    fn from(channel: Channel) -> Self {
        channel.id()
    }
}

impl From<TextChannel> for Snowflake<Channel> {
    fn from(channel: TextChannel) -> Self {
        channel.id()
    }
}

impl From<&Channel> for Snowflake<Channel> {
    fn from(channel: &Channel) -> Self {
        channel.id()
    }
}

impl From<&TextChannel> for Snowflake<Channel> {
    fn from(channel: &TextChannel) -> Self {
        channel.id()
    }
}

impl From<&mut Channel> for Snowflake<Channel> {
    fn from(channel: &mut Channel) -> Self {
        channel.id()
    }
}

impl From<&mut TextChannel> for Snowflake<Channel> {
    fn from(channel: &mut TextChannel) -> Self {
        channel.id()
    }
}
