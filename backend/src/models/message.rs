use chrono::{DateTime, Utc};
use derive_builder::Builder;
use serde::Serialize;
use slice_group_by::GroupBy;
use tokio_stream::StreamExt;
use warp::{multipart::FormData, Buf};

use super::{
    appstate::APP,
    attachment::{Attachment, AttachmentLike, AttachmentT},
    errors::{BuilderError, ChatError},
    member::UserLike,
    rest::CreateMessage,
    snowflake::Snowflake,
    user::User,
};

/// Represents a message record stored in the database.
pub struct MessageRecord {
    pub id: i64,
    pub channel_id: i64,
    pub user_id: Option<i64>,
    pub content: String,
}

/// Represents a message record with associated author data as queried.
/// All associated author fields are optional because the author may have been deleted.
pub struct ExtendedMessageRecord {
    pub id: i64,
    pub channel_id: i64,
    pub content: Option<String>,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub attachment_id: Option<i32>,
    pub attachment_filename: Option<String>,
    pub attachment_content_type: Option<String>,
}

/// A chat message.
#[derive(Serialize, Debug, Clone, Builder)]
#[builder(setter(into), build_fn(validate = "Self::validate", error = "BuilderError"))]
pub struct Message {
    /// The id of the message.
    id: Snowflake,

    /// The id of the channel this message was sent in.
    channel_id: Snowflake,

    /// The author of the message. This may be none if the author has been deleted since.
    #[builder(setter(strip_option))]
    author: Option<UserLike>,

    /// A nonce that can be used by a client to determine if the message was sent.
    /// The nonce is not stored in the database and thus is not returned by REST calls.
    #[builder(default)]
    nonce: Option<String>,

    /// The content of the message.
    #[builder(default)]
    pub content: Option<String>,

    /// Attachments sent with this message.
    #[builder(default)]
    attachments: Vec<AttachmentLike>,
}

impl MessageBuilder {
    fn validate(&self) -> Result<(), String> {
        if self.content.is_none() && (self.attachments.is_none() || self.attachments.as_ref().unwrap().is_empty()) {
            Err("Message must have content or attachments".to_string())
        } else {
            Ok(())
        }
    }
}

impl Message {
    /// Create a new builder for a message.
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    /// Create a new message or messages from the given records. Multiple records are linked together by their ID.
    pub fn from_records(records: &[ExtendedMessageRecord]) -> Vec<Self> {
        if records.is_empty() {
            return Vec::new();
        }

        records
            .linear_group_by(|a, b| a.id == b.id)
            .map(|group| {
                let author = group[0].user_id.map(|user_id| {
                    UserLike::User(
                        User::builder()
                            .id(user_id)
                            .username(group[0].username.clone().unwrap()) // SAFETY: This is safe because user_id is not None.
                            .display_name(group[0].display_name.clone())
                            .build()
                            .expect("Failed to build user"),
                    )
                });

                let mut attachments = Vec::new();

                for record in group {
                    if let Ok(attachment) = record.try_into() {
                        attachments.push(AttachmentLike::Partial(attachment));
                    }
                }

                Self {
                    id: group[0].id.into(),
                    channel_id: group[0].channel_id.into(),
                    author,
                    content: group[0].content.clone(),
                    nonce: None,
                    attachments,
                }
            })
            .collect()
    }

    /// Create a new message from the given formdata. Assigns a new snowflake to the message.
    ///
    /// ## Errors
    ///
    /// * `anyhow::Error` - If the formdata is invalid
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    pub async fn from_formdata(author: UserLike, channel_id: Snowflake, mut form: FormData) -> Result<Self, ChatError> {
        let id = Snowflake::gen_new().await;
        let mut attachments: Vec<AttachmentLike> = Vec::new();
        let mut builder = Message::builder();

        builder.id(id).channel_id(channel_id).author(author);

        while let Some(part) = form.next().await {
            let Ok(mut part) = part else {
                tracing::warn!("Failed to read form-data part, this error should not happen!");
                continue // Unsure why this can fail
            };

            tracing::debug!("Form-data part: {:?}", part);

            if part.name() == "json" && part.content_type().is_some_and(|ct| ct == "application/json") {
                let Some(Ok(data)) = part.data().await else {
                    return Err(ChatError::MalformedFieldError("json".to_string()));
                };
                let payload = serde_json::from_slice::<CreateMessage>(data.chunk())?;
                builder.content(payload.content).nonce(payload.nonce.clone());
            } else {
                let attachment = Attachment::try_from_form_part(part, channel_id, id).await?;

                if attachments.iter().any(|a| a.id() == attachment.id()) {
                    return Err(ChatError::DuplicateFieldError("attachment.id".to_string()));
                }
                attachments.push(AttachmentLike::Full(attachment));
            }
        }

        builder.attachments(attachments).build().map_err(Into::into)
    }

    /// Turns all attachments into partial attachments, removing the attachment contents from memory.
    pub fn strip_attachment_contents(mut self) -> Self {
        self.attachments = self
            .attachments
            .into_iter()
            .map(|a| {
                if let AttachmentLike::Full(f) = a {
                    AttachmentLike::Partial(f.into())
                } else {
                    a
                }
            })
            .collect();
        self
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

    /// Retrieve a message and fetch its author from the database in one query.
    /// Attachment contents will not be retrieved from S3.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    pub async fn fetch(id: Snowflake) -> Option<Self> {
        let db = &APP.db.read().await;
        let id_i64: i64 = id.into();

        // SAFETY: Must use `query_as_unchecked` because `ExtendedMessageRecord`
        // contains `Option<T>` for all users fields and sqlx does not recognize this.
        let records = sqlx::query_as_unchecked!(
            ExtendedMessageRecord,
            "SELECT messages.*, users.username, users.display_name, attachments.id AS attachment_id, attachments.filename AS attachment_filename, attachments.content_type AS attachment_content_type
            FROM messages
            LEFT JOIN users ON messages.user_id = users.id
            LEFT JOIN attachments ON messages.id = attachments.message_id
            WHERE messages.id = $1",
            id_i64
        )
        .fetch_all(db.pool())
        .await
        .ok()?;

        Self::from_records(&records).pop()
    }

    /// Commit this message to the database. Uploads all attachments to S3.
    /// It is highly recommended to call [`Message::strip_attachment_contents`] after calling
    /// this method to remove the attachment contents from memory.
    pub async fn commit(&self) -> Result<(), ChatError> {
        let db = &APP.db.read().await;
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

        for attachment in &self.attachments {
            if let AttachmentLike::Full(f) = attachment {
                f.commit().await?;
            }
        }
        Ok(())
    }
}
