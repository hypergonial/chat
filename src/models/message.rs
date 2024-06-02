use axum::extract::Multipart;
use chrono::{DateTime, Utc};
use derive_builder::Builder;
use serde::Serialize;
use slice_group_by::GroupBy;

use super::{
    attachment::{Attachment, AttachmentLike, FullAttachment},
    avatar::{Avatar, PartialAvatar, UserAvatar},
    channel::Channel,
    errors::{BuildError, RESTError},
    member::UserLike,
    requests::CreateMessage,
    snowflake::Snowflake,
    state::Config,
    user::User,
};

/// Represents a message record stored in the database.
pub struct MessageRecord {
    pub id: Snowflake<Message>,
    pub channel_id: Snowflake<Channel>,
    pub user_id: Option<Snowflake<User>>,
    pub content: String,
}

/// Represents a message record with associated author data as queried.
/// All associated author fields are optional because the author may have been deleted.
pub struct ExtendedMessageRecord {
    pub id: i64,
    pub channel_id: i64,
    pub content: Option<String>,
    pub user_id: Option<Snowflake<User>>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_hash: Option<String>,
    pub attachment_id: Option<i32>,
    pub attachment_filename: Option<String>,
    pub attachment_content_type: Option<String>,
}

/// A chat message.
#[derive(Serialize, Debug, Clone, Builder)]
#[builder(setter(into), build_fn(validate = "Self::validate", error = "BuildError"))]
pub struct Message {
    /// The id of the message.
    id: Snowflake<Message>,

    /// The id of the channel this message was sent in.
    channel_id: Snowflake<Channel>,

    /// The author of the message. This may be none if the author has been deleted since.
    #[builder(setter(strip_option))]
    author: Option<UserLike>,

    /// A nonce that can be used by a client to determine if the message was sent.
    /// The nonce is not stored in the database and thus is not returned by REST calls.
    #[builder(default)]
    nonce: Option<String>,

    /// The content of the message.
    #[builder(default)]
    content: Option<String>,

    /// Attachments sent with this message.
    #[builder(default)]
    attachments: Vec<Attachment>,
}

impl MessageBuilder {
    fn validate(&self) -> Result<(), String> {
        if self.content.is_none()
            && (self.attachments.is_none() || self.attachments.as_ref().is_some_and(Vec::is_empty))
        {
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

    /// The unique ID of this message.
    pub const fn id(&self) -> Snowflake<Self> {
        self.id
    }

    /// The user who sent this message.
    ///
    /// This may be `None` if the author has been deleted since.
    pub const fn author(&self) -> Option<&UserLike> {
        self.author.as_ref()
    }

    pub const fn channel_id(&self) -> Snowflake<Channel> {
        self.channel_id
    }

    /// The time at which this message was sent.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.id.created_at()
    }

    /// A nonce that can be used by a client to determine if the message was sent.
    /// The nonce is not stored in the database and thus is not returned by REST calls.
    pub const fn nonce(&self) -> Option<&String> {
        self.nonce.as_ref()
    }

    /// The content of the message.
    pub const fn content(&self) -> Option<&String> {
        self.content.as_ref()
    }

    /// Mutable handle to the content of the message.
    pub fn content_mut(&mut self) -> Option<&mut String> {
        self.content.as_mut()
    }

    /// The attachments sent with this message.
    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }

    /// Create a new message or messages from the given records. Multiple records are linked together by their ID.
    ///
    /// ## Errors
    ///
    /// * [`BuildError`] - If the records are invalid
    pub fn from_records(records: &[ExtendedMessageRecord]) -> Result<Vec<Self>, BuildError> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        records
            .linear_group_by(|a, b| a.id == b.id)
            .map(|group| {
                let author = {
                    if let Some(user_id) = group[0].user_id {
                        let avatar: Option<Avatar<UserAvatar>> = group[0]
                            .avatar_hash
                            .clone()
                            .map(|h| PartialAvatar::new(h, user_id).map(Avatar::Partial))
                            .transpose()?;

                        let user = User::builder()
                            .id(user_id)
                            .username(group[0].username.clone().expect("User should have username")) // SAFETY: This is safe because user_id is not None.
                            .display_name(group[0].display_name.clone())
                            .avatar(avatar)
                            .build()?;
                        Some(UserLike::User(user))
                    } else {
                        None
                    }
                };

                let attachments = group
                    .iter()
                    .flat_map(TryInto::try_into)
                    .map(Attachment::Partial)
                    .collect();

                Ok(Self {
                    id: group[0].id.into(),
                    channel_id: group[0].channel_id.into(),
                    author,
                    content: group[0].content.clone(),
                    nonce: None,
                    attachments,
                })
            })
            .collect()
    }

    /// Create a new message from the given formdata. Assigns a new snowflake to the message.
    ///
    /// ## Errors
    ///
    /// * [`RESTError`] - If the formdata is invalid
    pub async fn from_formdata(
        config: &Config,
        author: UserLike,
        channel: impl Into<Snowflake<Channel>>,
        mut form: Multipart,
    ) -> Result<Self, RESTError> {
        let id = Snowflake::gen_new(config);
        let channel_id: Snowflake<Channel> = channel.into();
        let mut attachments: Vec<Attachment> = Vec::new();
        let mut builder = Self::builder();

        builder.id(id).channel_id(channel_id).author(author);

        while let Some(part) = form.next_field().await? {
            tracing::debug!("Form-data part: {:?}", part);

            if part.name() == Some("json") && part.content_type().is_some_and(|ct| ct == "application/json") {
                let Ok(data) = part.bytes().await else {
                    return Err(RESTError::MalformedField("json".to_string()));
                };
                let payload = serde_json::from_slice::<CreateMessage>(&data)?;
                builder.content(payload.content).nonce(payload.nonce.clone());
            } else {
                let attachment = FullAttachment::try_from_field(part, channel_id, id).await?;

                if attachments.iter().any(|a| a.id() == attachment.id()) {
                    return Err(RESTError::DuplicateField("attachment.id".to_string()));
                }
                attachments.push(Attachment::Full(attachment));
            }
        }

        Ok(builder.attachments(attachments).build()?)
    }

    /// Turns all attachments into partial attachments, removing the attachment contents from memory.
    #[must_use]
    pub fn strip_attachment_contents(mut self) -> Self {
        self.attachments = self
            .attachments
            .into_iter()
            .map(|a| {
                if let Attachment::Full(f) = a {
                    Attachment::Partial(f.into())
                } else {
                    a
                }
            })
            .collect();
        self
    }
}

impl From<Message> for Snowflake<Message> {
    fn from(message: Message) -> Self {
        message.id()
    }
}

impl From<&Message> for Snowflake<Message> {
    fn from(message: &Message) -> Self {
        message.id()
    }
}
