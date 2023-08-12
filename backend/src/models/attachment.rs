use super::{appstate::APP, errors::ChatError, message::ExtendedMessageRecord};
use derive_builder::Builder;
use enum_dispatch::enum_dispatch;
use lazy_static::lazy_static;
use regex::Regex;
use s3::error::S3Error;
use serde::Serialize;
use warp::multipart::Part;

use super::snowflake::Snowflake;

lazy_static! {
    static ref ATTACHMENT_REGEX: Regex = Regex::new(r"attachment-(?P<id>[0-9])").unwrap();
}

/// Trait used for enum dispatch
#[enum_dispatch(AttachmentLike)]
trait AttachmentT {
    /// The ID of the attachment.
    /// This determines the ordering of attachments within a message, starting from 0.
    fn id(&self) -> u8;
    /// The name of the attachment file, including the file extension.
    fn filename(&self) -> &String;
    /// The ID of the message this attachment belongs to.
    fn message_id(&self) -> Snowflake;
}

/// An object representing either a partial or full attachment.
/// In practice, both should serialize identically, the only difference is that a
/// partial attachment does not have the content loaded.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[enum_dispatch]
pub enum AttachmentLike {
    Full(Attachment),
    Partial(PartialAttachment),
}

#[derive(Debug, Clone, Builder, Serialize)]
pub struct Attachment {
    /// Describes the ordering of attachments within a message, starting from 0.
    id: u8,
    /// The name of the attachment file, including the file extension.
    filename: String,
    /// The contents of the file.
    #[serde(skip)]
    content: Vec<u8>,
    /// The ID of the message this attachment belongs to.
    #[serde(skip)]
    message_id: Snowflake,
}

impl Attachment {
    /// Create a new attachment with the given ID, filename, and content.
    pub fn new(id: u8, filename: String, content: Vec<u8>, message_id: Snowflake) -> Self {
        Self {
            id,
            filename,
            content,
            message_id,
        }
    }

    pub fn builder() -> AttachmentBuilder {
        AttachmentBuilder::default()
    }

    pub fn try_from_form_part(part: Part, message_id: Snowflake) -> anyhow::Result<Self> {
        let Some(caps) = ATTACHMENT_REGEX.captures(part.name()) else {
            anyhow::bail!("Invalid attachment name: {}", part.name());
        };
        let Some(filename) = part.filename() else {
            anyhow::bail!("Missing filename for attachment: {}", part.name());
        };
        Self::builder()
            .id(caps["id"].parse()?)
            .filename(filename.to_string())
            .message_id(message_id)
            .build()
            .map_err(Into::into)
    }

    pub fn s3_path(&self) -> String {
        format!("{}/{}/{}", self.message_id, self.id, self.filename)
    }

    /// Commit the attachment to the database. Uploads the contents to S3 implicitly.
    pub async fn commit(&self) -> Result<(), ChatError> {
        let db = &APP.db.read().await;
        let message_id: i64 = self.message_id.into();

        sqlx::query!(
            "INSERT INTO attachments (id, filename, message_id)
            VALUES ($1, $2, $3) ON CONFLICT (id, message_id) DO UPDATE SET filename = $2",
            self.id as i32,
            self.filename,
            message_id
        )
        .execute(db.pool())
        .await?;

        self.upload().await?;

        Ok(())
    }

    /// Upload the attachment content to S3. This function is called implicitly by [`Attachment`]`::commit`.
    pub async fn upload(&self) -> Result<(), S3Error> {
        let bucket = APP.buckets().attachments();
        bucket.put_object(self.s3_path(), &self.content).await?;
        Ok(())
    }

    /// Download the attachment content from S3.
    pub async fn download(&mut self) -> Result<(), S3Error> {
        let bucket = APP.buckets().attachments();
        self.content = bucket.get_object(self.s3_path()).await?.to_vec();
        Ok(())
    }

    /// Delete the contents of the attachment from S3.
    /// This should be called after the attachment is deleted from the database.
    pub async fn delete(&self) -> Result<(), S3Error> {
        let bucket = APP.buckets().attachments();
        bucket.delete_object(self.s3_path()).await?;
        Ok(())
    }
}

impl AttachmentT for Attachment {
    fn id(&self) -> u8 {
        self.id
    }

    fn filename(&self) -> &String {
        &self.filename
    }

    fn message_id(&self) -> Snowflake {
        self.message_id
    }
}

/// A partial attachment, as stored in the database.
pub struct PartialAttachmentRecord {
    id: i32,
    filename: String,
    message_id: i64,
}

/// A partial attachment, with the binary content not loaded.
#[derive(Debug, Clone, Builder, Serialize)]
pub struct PartialAttachment {
    /// Describes the ordering of attachments within a message, starting from 0.
    id: u8,
    /// The name of the attachment file, including the file extension.
    filename: String,
    /// The ID of the message this attachment belongs to.
    #[serde(skip)]
    message_id: Snowflake,
}

impl PartialAttachment {
    /// Create a new partial attachment with the given ID and filename.
    pub fn new(id: u8, filename: String, message_id: Snowflake) -> Self {
        Self {
            id,
            filename,
            message_id,
        }
    }

    /// Download the attachment content from S3, turning this into a full attachment.
    pub async fn download(self) -> Attachment {
        let mut attachment = Attachment::new(self.id, self.filename, Vec::new(), self.message_id);
        attachment.download().await.unwrap();
        attachment
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
    /// * `APP.db` (read)
    pub async fn fetch(id: u8, message_id: Snowflake) -> Result<Option<Self>, sqlx::Error> {
        let db = APP.db.read().await;
        let message_id: i64 = message_id.into();

        Ok(sqlx::query_as!(
            PartialAttachmentRecord,
            "SELECT id, filename, message_id
            FROM attachments
            WHERE id = $1 AND message_id = $2",
            id as i32,
            message_id
        )
        .fetch_optional(db.pool())
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
    /// * `APP.db` (read)
    pub async fn fetch_all(message_id: Snowflake) -> Result<Vec<Self>, sqlx::Error> {
        let db = APP.db.read().await;
        let message_id: i64 = message_id.into();

        Ok(sqlx::query_as!(
            PartialAttachmentRecord,
            "SELECT id, filename, message_id
            FROM attachments
            WHERE message_id = $1",
            message_id
        )
        .fetch_all(db.pool())
        .await?
        .into_iter()
        .map(|record| record.into())
        .collect())
    }
}

impl From<Attachment> for PartialAttachment {
    fn from(attachment: Attachment) -> Self {
        Self {
            id: attachment.id,
            filename: attachment.filename,
            message_id: attachment.message_id,
        }
    }
}

impl From<PartialAttachmentRecord> for PartialAttachment {
    fn from(record: PartialAttachmentRecord) -> Self {
        Self {
            id: record.id as u8,
            filename: record.filename,
            message_id: Snowflake::from(record.message_id),
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
            id: id.try_into().unwrap(),
            message_id: record.id.into(),
            filename: filename.clone(),
        })
    }
}

impl AttachmentT for PartialAttachment {
    fn id(&self) -> u8 {
        self.id
    }

    fn filename(&self) -> &String {
        &self.filename
    }

    fn message_id(&self) -> Snowflake {
        self.message_id
    }
}
