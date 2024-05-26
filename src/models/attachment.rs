use std::sync::OnceLock;

use super::{
    appstate::app,
    errors::{AppError, BuilderError, RESTError},
    message::ExtendedMessageRecord,
};
use axum::extract::multipart::Field;
use bytes::Bytes;
use derive_builder::Builder;
use enum_dispatch::enum_dispatch;
use mime::Mime;
use regex::Regex;
use serde::Serialize;

use super::snowflake::Snowflake;

fn attachment_regex() -> &'static Regex {
    static ATTACH_REGEX: OnceLock<Regex> = OnceLock::new();
    ATTACH_REGEX.get_or_init(|| Regex::new(r"attachment-(?P<id>[0-9])").unwrap())
}

/// Trait used for enum dispatch
#[enum_dispatch(Attachment)]
pub trait AttachmentLike {
    /// The ID of the attachment.
    /// This determines the ordering of attachments within a message, starting from 0.
    fn id(&self) -> u8;
    /// The name of the attachment file, including the file extension.
    fn filename(&self) -> &String;
    /// The ID of the message this attachment belongs to.
    fn message_id(&self) -> Snowflake;
    /// The ID of the channel the message was sent to.
    fn channel_id(&self) -> Snowflake;
    /// The MIME-type of the file.
    fn mime(&self) -> Mime;
    /// The path to the attachment in S3.
    fn s3_key(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.channel_id(),
            self.message_id(),
            self.id(),
            self.filename()
        )
    }
}

/// An object representing either a partial or full attachment.
/// In practice, both should serialize identically, the only difference is that a
/// partial attachment does not have the content loaded.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[enum_dispatch]
pub enum Attachment {
    Full(FullAttachment),
    Partial(PartialAttachment),
}

#[derive(Debug, Clone, Builder, Serialize)]
#[builder(setter(into), build_fn(error = "BuilderError"))]
pub struct FullAttachment {
    /// Describes the ordering of attachments within a message, starting from 0.
    id: u8,
    /// The name of the attachment file, including the file extension.
    filename: String,
    /// The contents of the file.
    #[serde(skip)]
    content: Bytes,
    /// The MIME type of the file.
    content_type: String,
    /// The ID of the message this attachment belongs to.
    #[serde(skip)]
    message_id: Snowflake,
    /// The ID of the channel the message was sent to.
    #[serde(skip)]
    channel_id: Snowflake,
}

impl FullAttachment {
    /// Create a new attachment with the given ID, filename, and content.
    pub fn new(
        id: u8,
        filename: String,
        content: impl Into<Bytes>,
        content_type: String,
        channel: impl Into<Snowflake>,
        message: impl Into<Snowflake>,
    ) -> Self {
        Self {
            id,
            filename,
            content: content.into(),
            content_type,
            channel_id: channel.into(),
            message_id: message.into(),
        }
    }

    pub fn builder() -> FullAttachmentBuilder {
        FullAttachmentBuilder::default()
    }

    /// Try to build a new [`Attachment`] from a multipart/form-data field.
    ///
    /// ## Arguments
    ///
    /// * `field` - The field to build from.
    /// * `channel` - The ID of the channel the message was sent to.
    /// * `message` - The ID of the message this attachment belongs to.
    ///
    /// ## Returns
    ///
    /// [`Attachment`] - The built attachment.
    ///
    /// ## Errors
    ///
    /// * [`RESTError::MissingField`] - If a required field is missing.
    /// * [`RESTError::MalformedField`] - If the attachment ID could not be parsed from the field name.
    /// * [`RESTError::App`] - If the field contents could not be read.
    pub async fn try_from_field(
        field: Field<'_>,
        channel: impl Into<Snowflake>,
        message: impl Into<Snowflake>,
    ) -> Result<Self, RESTError> {
        let mut builder = FullAttachment::builder();

        let Some(name) = field.name() else {
            return Err(RESTError::MissingField("name".into()));
        };

        let Some(filename) = field.file_name() else {
            return Err(RESTError::MissingField("filename".into()));
        };

        builder.filename(filename);

        let Some(caps) = attachment_regex().captures(name) else {
            return Err(RESTError::MalformedField(
                "attachment ID could not be parsed from name".into(),
            ));
        };
        builder.id(caps["id"]
            .parse::<u8>()
            .expect("attachment ID should have been a valid number"));

        let content_type = field.content_type().unwrap_or("application/octet-stream");

        // Ensure the content type is valid
        content_type
            .parse::<Mime>()
            .map_err(|_| RESTError::MalformedField("content type could not be parsed".into()))?;

        Ok(builder
            .channel_id(channel)
            .message_id(message)
            .content_type(content_type)
            .content(field.bytes().await?)
            .build()?)
    }

    /// Commit the attachment to the database. Uploads the contents to S3 implicitly.
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    pub async fn commit(&self) -> Result<(), AppError> {
        let message_id: i64 = self.message_id.into();
        let channel_id: i64 = self.channel_id.into();

        self.upload().await?;

        sqlx::query!(
            "INSERT INTO attachments (id, filename, message_id, channel_id, content_type)
            VALUES ($1, $2, $3, $4, $5) 
            ON CONFLICT (id, message_id) 
            DO UPDATE SET filename = $2, content_type = $5",
            self.id as i32,
            self.filename,
            message_id,
            channel_id,
            self.content_type,
        )
        .execute(app().db.pool())
        .await?;

        Ok(())
    }

    /// Upload the attachment content to S3. This function is called implicitly by [`Attachment`]`::commit`.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn upload(&self) -> Result<(), AppError> {
        let bucket = app().buckets.attachments();
        bucket
            .put_object(&app().s3, self.s3_key(), self.content.clone(), self.mime())
            .await
    }

    /// Download the attachment content from S3.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn download(&mut self) -> Result<(), AppError> {
        let bucket = app().buckets.attachments();
        self.content = bucket.get_object(&app().s3, self.s3_key()).await?;
        Ok(())
    }

    /// Delete the contents of the attachment from S3.
    /// This should be called after the attachment is deleted from the database.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn delete(&self) -> Result<(), AppError> {
        let bucket = app().buckets.attachments();
        bucket.delete_object(&app().s3, self.s3_key()).await
    }
}

impl AttachmentLike for FullAttachment {
    fn id(&self) -> u8 {
        self.id
    }

    fn filename(&self) -> &String {
        &self.filename
    }

    fn channel_id(&self) -> Snowflake {
        self.channel_id
    }

    fn message_id(&self) -> Snowflake {
        self.message_id
    }

    fn mime(&self) -> Mime {
        self.content_type.parse().expect("Invalid MIME type")
    }
}

/// A partial attachment, as stored in the database.
pub struct PartialAttachmentRecord {
    id: i32,
    filename: String,
    message_id: i64,
    content_type: String,
}

/// A partial attachment, with the binary content not loaded.
#[derive(Debug, Clone, Builder, Serialize)]
#[builder(setter(into), build_fn(error = "BuilderError"))]
pub struct PartialAttachment {
    /// Describes the ordering of attachments within a message, starting from 0.
    id: u8,
    /// The name of the attachment file, including the file extension.
    filename: String,
    /// The MIME type of the file.
    content_type: String,
    /// The ID of the message this attachment belongs to.
    #[serde(skip)]
    message_id: Snowflake,
    /// The ID of the channel the message was sent to.
    #[serde(skip)]
    channel_id: Snowflake,
}

impl PartialAttachment {
    /// Create a new partial attachment with the given ID and filename.
    pub fn new(
        id: u8,
        filename: String,
        content_type: String,
        channel: impl Into<Snowflake>,
        message: impl Into<Snowflake>,
    ) -> Self {
        Self {
            id,
            filename,
            content_type,
            channel_id: channel.into(),
            message_id: message.into(),
        }
    }

    /// Download the attachment content from S3, turning this into a full attachment.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn download(self) -> Result<FullAttachment, AppError> {
        let mut attachment = FullAttachment::new(
            self.id,
            self.filename,
            Vec::new(),
            self.content_type,
            self.channel_id,
            self.message_id,
        );
        attachment.download().await?;
        Ok(attachment)
    }

    /// Fetches a single attachment from the database.
    ///
    /// ## Arguments
    ///
    /// * `id` - The ID of the attachment to fetch
    /// * `message_id` - The ID of the message this attachment belongs to
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the SQL query fails.
    pub async fn fetch(id: u8, message: impl Into<Snowflake>) -> Result<Option<Self>, sqlx::Error> {
        let message_id: i64 = message.into().into();

        Ok(sqlx::query_as!(
            PartialAttachmentRecord,
            "SELECT id, filename, message_id, content_type
            FROM attachments
            WHERE id = $1 AND message_id = $2",
            id as i32,
            message_id
        )
        .fetch_optional(app().db.pool())
        .await?
        .map(|record| record.into()))
    }

    /// Fetches all attachments belonging to a message from the database.
    ///
    /// ## Arguments
    ///
    /// * `message_id` - The ID of the message to fetch attachments for
    ///
    /// ## Locks
    ///
    /// * `app().db` (read)
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the SQL query fails.
    pub async fn fetch_all(message: impl Into<Snowflake>) -> Result<Vec<Self>, sqlx::Error> {
        let message_id: i64 = message.into().into();

        Ok(sqlx::query_as!(
            PartialAttachmentRecord,
            "SELECT id, filename, message_id, content_type
            FROM attachments
            WHERE message_id = $1",
            message_id
        )
        .fetch_all(app().db.pool())
        .await?
        .into_iter()
        .map(|record| record.into())
        .collect())
    }
}

impl From<FullAttachment> for PartialAttachment {
    fn from(attachment: FullAttachment) -> Self {
        Self {
            id: attachment.id,
            filename: attachment.filename,
            channel_id: attachment.channel_id,
            message_id: attachment.message_id,
            content_type: attachment.content_type,
        }
    }
}

impl From<PartialAttachmentRecord> for PartialAttachment {
    fn from(record: PartialAttachmentRecord) -> Self {
        Self {
            id: record.id as u8,
            filename: record.filename,
            channel_id: Snowflake::from(record.message_id),
            message_id: Snowflake::from(record.message_id),
            content_type: record.content_type,
        }
    }
}

impl TryFrom<&ExtendedMessageRecord> for PartialAttachment {
    type Error = String;

    fn try_from(record: &ExtendedMessageRecord) -> Result<Self, Self::Error> {
        let id = record.attachment_id.ok_or("No attachment ID".to_string())?;
        let filename = record
            .attachment_filename
            .as_ref()
            .ok_or("No attachment filename".to_string())?;
        Ok(Self {
            id: id.try_into().expect("attachment ID should be a single positive digit"),
            channel_id: record.channel_id.into(),
            message_id: record.id.into(),
            filename: filename.clone(),
            content_type: record
                .attachment_content_type
                .clone()
                .unwrap_or("application/octet-stream".into()),
        })
    }
}

impl AttachmentLike for PartialAttachment {
    fn id(&self) -> u8 {
        self.id
    }

    fn filename(&self) -> &String {
        &self.filename
    }

    fn channel_id(&self) -> Snowflake {
        self.channel_id
    }

    fn message_id(&self) -> Snowflake {
        self.message_id
    }

    fn mime(&self) -> Mime {
        self.content_type.parse().unwrap()
    }
}
