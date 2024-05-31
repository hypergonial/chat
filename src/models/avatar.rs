use std::fmt::Debug;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::{
    bucket::{Bucket, Buckets},
    data_uri::DataUri,
    errors::{AppError, BuildError},
    guild::Guild,
    user::User,
};
use bytes::Bytes;
use derive_builder::Builder;
use mime::Mime;
use serde::{Deserialize, Serialize};

use super::snowflake::Snowflake;

/// The file extension of the avatar.
fn mime_to_img_ext(mime: &Mime) -> String {
    let mime_str = mime.to_string();
    let mut mime = mime_str.splitn(2, '/');
    let type_ = mime.next().expect("MIME type should have a type");
    let subtype = mime.next().expect("MIME type should have a subtype");

    if type_ != "image" {
        return "bin".into();
    }
    subtype.into()
}

/// Check if a MIME type is an image.
fn is_mime_image(mime: &Mime) -> bool {
    mime.type_() == "image"
}

/// Represents the kind of avatar resource.
pub trait AvatarKind: Debug + Default + Clone + Copy + PartialEq + Eq
where
    Self::HolderType: Debug + Clone + PartialEq + Eq,
{
    /// The kind of object that holds this avatar.
    type HolderType;

    /// The bucket this kind of avatar is stored in.
    fn bucket(&self) -> &'static str;
}

/// Represents a guild's icon
#[derive(Debug, Clone, Serialize, Deserialize, Default, Copy, PartialEq, Eq)]
pub struct GuildAvatar;

impl AvatarKind for GuildAvatar {
    type HolderType = Guild;

    #[inline]
    fn bucket(&self) -> &'static str {
        "guilds"
    }
}

/// Represents a user's profile picture
#[derive(Debug, Clone, Serialize, Deserialize, Default, Copy, PartialEq, Eq)]
pub struct UserAvatar;

impl AvatarKind for UserAvatar {
    type HolderType = User;

    #[inline]
    fn bucket(&self) -> &'static str {
        "users"
    }
}

pub trait AvatarLike<K: AvatarKind> {
    /// The hash of the avatar. This should end in the file extension.
    fn avatar_hash(&self) -> &str;
    /// The id of the object that has this avatar.
    fn holder_id(&self) -> Snowflake<K::HolderType>;
    /// The MIME type of the attachment.
    fn mime(&self) -> &Mime;

    /// The kind of avatar this is.
    fn kind(&self) -> K {
        K::default()
    }

    /// The bucket this avatar is stored in S3.
    fn bucket<'a>(&self, s3: &'a Buckets) -> Bucket<'a> {
        s3.get_bucket(self.kind().bucket())
    }

    /// The path to the attachment in S3.
    fn s3_key(&self) -> String {
        format!(
            "{}/{}.{}",
            self.holder_id(),
            self.avatar_hash(),
            mime_to_img_ext(self.mime())
        )
    }

    /// Delete the contents of the attachment from S3.
    /// This should be called after the attachment is deleted from the database.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    async fn delete(&self, s3: &Buckets) -> Result<(), AppError> {
        self.bucket(s3).delete_object(self.s3_key()).await
    }
}

/// An object representing an avatar.
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
#[serde(untagged)]
pub enum Avatar<K: AvatarKind> {
    Full(FullAvatar<K>),
    Partial(PartialAvatar<K>),
}

impl<K: AvatarKind> AvatarLike<K> for Avatar<K> {
    fn avatar_hash(&self) -> &str {
        match self {
            Self::Full(avatar) => avatar.avatar_hash(),
            Self::Partial(avatar) => avatar.avatar_hash(),
        }
    }

    fn holder_id(&self) -> Snowflake<K::HolderType> {
        match self {
            Self::Full(avatar) => avatar.holder_id(),
            Self::Partial(avatar) => avatar.holder_id(),
        }
    }

    fn mime(&self) -> &Mime {
        match self {
            Self::Full(avatar) => avatar.mime(),
            Self::Partial(avatar) => avatar.mime(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Builder)]
#[builder(setter(into), build_fn(validate = "Self::validate", error = "BuildError"))]
pub struct FullAvatar<K: AvatarKind> {
    /// The hash of the avatar.
    avatar_hash: String,
    /// The ID of the message this attachment belongs to.
    holder_id: Snowflake<K::HolderType>,
    /// The contents of the file.
    content: Bytes,
    /// The MIME type of the file.
    mime: Mime,
}

impl<K: AvatarKind> FullAvatarBuilder<K> {
    fn validate(&self) -> Result<(), String> {
        let Some(mime) = self.mime.as_ref() else {
            return Err("MIME type is required".to_string());
        };
        if !is_mime_image(mime) {
            return Err("MIME type must be an image".to_string());
        }
        Ok(())
    }
}

impl<K: AvatarKind> FullAvatar<K> {
    pub fn builder() -> FullAvatarBuilder<K> {
        FullAvatarBuilder::default()
    }

    /// Build a new avatar from a data URI.
    ///
    /// ## Errors
    ///
    /// * If the MIME type is not an image.
    pub fn from_data_uri(user: impl Into<Snowflake<K::HolderType>>, uri: DataUri) -> Result<Self, BuildError> {
        let mime = uri.mime().clone();
        let mut hasher = DefaultHasher::new();
        uri.hash(&mut hasher);
        let avatar_hash = format!("{}_{}", hasher.finish(), mime_to_img_ext(&mime));

        Self::builder()
            .holder_id(user)
            .mime(uri.mime().clone())
            .content(uri)
            .avatar_hash(avatar_hash)
            .build()
    }

    /// Upload the avatar content to S3. This function is called implicitly if the user is updated.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn upload(&self, s3: &Buckets) -> Result<(), AppError> {
        self.bucket(s3)
            .put_object(self.s3_key(), self.content.clone(), self.mime())
            .await
    }

    /// Download the avatar content from S3.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn download(&mut self, s3: &Buckets) -> Result<(), AppError> {
        self.content = self.bucket(s3).get_object(self.s3_key()).await?;
        Ok(())
    }
}

impl<K: AvatarKind> Serialize for FullAvatar<K> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.avatar_hash)
    }
}

impl<K: AvatarKind> AvatarLike<K> for FullAvatar<K> {
    fn avatar_hash(&self) -> &str {
        &self.avatar_hash
    }

    fn holder_id(&self) -> Snowflake<K::HolderType> {
        self.holder_id
    }

    fn mime(&self) -> &Mime {
        &self.mime
    }
}

/// A partial avatar, with the binary content not loaded.
#[derive(Debug, PartialEq, Eq, Clone, Builder)]
#[builder(setter(into), build_fn(error = "BuildError"))]
pub struct PartialAvatar<K: AvatarKind> {
    /// The hash of the avatar.
    avatar_hash: String,
    /// The ID of the message this attachment belongs to.
    holder_id: Snowflake<K::HolderType>,
    /// The MIME type of the file.
    mime: Mime,
}

impl<K: AvatarKind> PartialAvatar<K> {
    /// Create a new partial avatar with the given hash and holder ID.
    ///
    /// ## Errors
    ///
    /// * If the MIME type is not an image.
    pub fn new(avatar_hash: String, holder_id: impl Into<Snowflake<K::HolderType>>) -> Result<Self, BuildError> {
        let mime = {
            avatar_hash.split('_').last().map_or_else(
                || Err(BuildError::ValidationError("no MIME type at end of avatar hash".into())),
                |file_ext| match file_ext {
                    "png" => Ok(mime::IMAGE_PNG),
                    "jpg" | "jpeg" => Ok(mime::IMAGE_JPEG),
                    "gif" => Ok(mime::IMAGE_GIF),
                    "bmp" => Ok(mime::IMAGE_BMP),
                    "webp" => Ok("image/webp".parse().expect("image/webp should be a valid MIME")),
                    _ => Err(BuildError::ValidationError("invalid file extension".into())),
                },
            )
        }?;

        Ok(Self {
            avatar_hash,
            holder_id: holder_id.into(),
            mime,
        })
    }

    /// Download the avatar content from S3, turning this into a full avatar.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn download(self, buckets: &Buckets) -> Result<FullAvatar<K>, AppError> {
        let mime = self.mime().clone();
        let mut attachment = FullAvatar::builder()
            .avatar_hash(self.avatar_hash)
            .holder_id(self.holder_id)
            .mime(mime)
            .content(Bytes::new())
            .build()?;
        attachment.download(buckets).await?;
        Ok(attachment)
    }
}

impl<K: AvatarKind> From<FullAvatar<K>> for PartialAvatar<K> {
    fn from(value: FullAvatar<K>) -> Self {
        Self {
            avatar_hash: value.avatar_hash,
            holder_id: value.holder_id,
            mime: value.mime,
        }
    }
}

impl<K: AvatarKind> AvatarLike<K> for PartialAvatar<K> {
    fn avatar_hash(&self) -> &str {
        &self.avatar_hash
    }

    fn mime(&self) -> &Mime {
        &self.mime
    }

    fn holder_id(&self) -> Snowflake<K::HolderType> {
        self.holder_id
    }
}

impl<K: AvatarKind> Serialize for PartialAvatar<K> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.avatar_hash)
    }
}
