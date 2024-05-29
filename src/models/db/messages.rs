use crate::models::{
    attachment::Attachment,
    errors::AppError,
    message::{ExtendedMessageRecord, Message},
    snowflake::Snowflake,
};

use super::Database;

#[derive(Debug, Clone)]
pub struct MessagesHandler<'a> {
    db: &'a Database,
}

impl<'a> MessagesHandler<'a> {
    pub const fn new(db: &'a Database) -> Self {
        MessagesHandler { db }
    }

    /// Retrieve a message and fetch its author from the database in one query.
    /// Attachment contents will not be retrieved from S3.
    ///
    /// ## Arguments
    ///
    /// * `message` - The ID of the message to retrieve.
    ///
    /// ## Returns
    ///
    /// The message if found, otherwise `None`.
    pub async fn fetch_message(&self, message: impl Into<Snowflake<Message>>) -> Option<Message> {
        let id_i64: i64 = message.into().into();

        let records = sqlx::query_as!(
            ExtendedMessageRecord,
            "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
            FROM messages
            LEFT JOIN users ON messages.user_id = users.id
            LEFT JOIN attachments ON messages.id = attachments.message_id
            WHERE messages.id = $1",
            id_i64
        )
        .fetch_all(self.db.pool())
        .await
        .ok()?;

        Message::from_records(&records).pop()
    }

    /// Commit this message to the database. Uploads all attachments to S3.
    /// It is highly recommended to call [`Message::strip_attachment_contents`] after calling
    /// this method to remove the attachment contents from memory.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request to upload one of the attachments fails.
    /// * [`AppError::Database`] - If the database request fails.
    pub async fn update_message(&self, message: &Message) -> Result<(), AppError> {
        let id_i64: i64 = message.id().into();
        let author_id_i64: Option<i64> = message.author().map(|u| u.id().into());
        let channel_id_i64: i64 = message.channel_id().into();
        sqlx::query!(
            "INSERT INTO messages (id, user_id, channel_id, content)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET user_id = $2, channel_id = $3, content = $4",
            id_i64,
            author_id_i64,
            channel_id_i64,
            message.content(),
        )
        .execute(self.db.pool())
        .await?;

        for attachment in message.attachments() {
            if let Attachment::Full(f) = attachment {
                f.commit(self.db).await?;
            }
        }
        Ok(())
    }
}
