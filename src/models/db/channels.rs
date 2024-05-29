use crate::models::{
    channel::{Channel, ChannelLike, ChannelRecord},
    errors::AppError,
    message::{ExtendedMessageRecord, Message},
    snowflake::Snowflake,
};

use super::Database;

#[derive(Debug, Clone)]
pub struct ChannelsHandler<'a> {
    db: &'a Database,
}

impl<'a> ChannelsHandler<'a> {
    pub const fn new(db: &'a Database) -> Self {
        ChannelsHandler { db }
    }

    pub async fn fetch_channel(&self, id: impl Into<Snowflake<Channel>>) -> Option<Channel> {
        let id_64: i64 = id.into().into();

        let record = sqlx::query_as!(ChannelRecord, "SELECT * FROM channels WHERE id = $1", id_64)
            .fetch_optional(self.db.pool())
            .await
            .ok()??;

        Some(Channel::from_record(record))
    }

    /// Create a new channel in the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn create_channel(&self, general: &Channel) -> Result<(), sqlx::Error> {
        let id_64: i64 = general.id().into();
        let guild_id_64: i64 = general.guild_id().into();
        sqlx::query!(
            "INSERT INTO channels (id, guild_id, name, channel_type)
            VALUES ($1, $2, $3, $4)",
            id_64,
            guild_id_64,
            general.name(),
            general.channel_type(),
        )
        .execute(self.db.pool())
        .await?;

        Ok(())
    }

    /// Commit this channel to the database.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database query fails.
    pub async fn update_channel(&self, channel: &Channel) -> Result<(), sqlx::Error> {
        let id_64: i64 = channel.id().into();
        sqlx::query!("UPDATE channels SET name = $2 WHERE id = $1", id_64, channel.name(),)
            .execute(self.db.pool())
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
    pub async fn delete_channel(&mut self, channel: impl Into<Snowflake<Channel>>) -> Result<(), AppError> {
        let channel_id: Snowflake<Channel> = channel.into();

        self.db.app().buckets.remove_all_for_channel(channel_id).await?;

        let id_i64: i64 = channel_id.into();

        sqlx::query!("DELETE FROM channels WHERE id = $1", id_i64)
            .execute(self.db.pool())
            .await?;

        Ok(())
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
    pub async fn fetch_messages_from(
        &self,
        channel: impl Into<Snowflake<Channel>>,
        limit: Option<u32>,
        before: Option<Snowflake<Message>>,
        after: Option<Snowflake<Message>>,
    ) -> Result<Vec<Message>, sqlx::Error> {
        let limit = limit.unwrap_or(50).min(100);

        let id_64: i64 = channel.into().into();

        let records: Vec<ExtendedMessageRecord> = if before.is_none() && after.is_none() {
            sqlx::query_as!(
                ExtendedMessageRecord,
                "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
                FROM messages
                LEFT JOIN users ON messages.user_id = users.id
                LEFT JOIN attachments ON messages.id = attachments.message_id
                WHERE messages.channel_id = $1
                ORDER BY messages.id DESC LIMIT $2",
                id_64,
                i64::from(limit)
            )
            .fetch_all(self.db.pool())
            .await?
        } else {
            sqlx::query_as!(
                ExtendedMessageRecord,
                "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
                FROM messages
                LEFT JOIN users ON messages.user_id = users.id
                LEFT JOIN attachments ON messages.id = attachments.message_id
                WHERE messages.channel_id = $1 AND messages.id > $2 AND messages.id < $3
                ORDER BY messages.id DESC LIMIT $4",
                id_64,
                before.map_or(i64::MAX, Into::into),
                after.map_or(i64::MIN, Into::into),
                i64::from(limit)
            )
            .fetch_all(self.db.pool())
            .await?
        };
        Ok(Message::from_records(&records))
    }
}
