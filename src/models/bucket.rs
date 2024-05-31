use std::sync::{Arc, Weak};

use aws_sdk_s3::{
    primitives::ByteStream,
    types::{Delete, Object, ObjectIdentifier},
    Client,
};
use bytes::{Bytes, BytesMut};
use mime::Mime;

use super::{channel::Channel, errors::AppError, guild::Guild, snowflake::Snowflake, state::ApplicationState};

pub type S3Client = Client;

/// All S3 buckets used by the application.
#[derive(Debug, Clone)]
pub struct Buckets {
    app: Weak<ApplicationState>,
    client: S3Client,
}

impl Buckets {
    /// Create all buckets from the given config.
    pub fn new(client: S3Client) -> Self {
        Self {
            client,
            app: Weak::new(),
        }
    }

    pub fn bind_to(&mut self, app: Weak<ApplicationState>) {
        self.app = app;
    }

    pub fn app(&self) -> Arc<ApplicationState> {
        self.app.upgrade().expect("Application state has been dropped.")
    }

    pub const fn client(&self) -> &S3Client {
        &self.client
    }

    const fn get_bucket(&self, name: &'static str) -> Bucket {
        Bucket::new(self, name)
    }

    /// The attachments bucket.
    /// It is responsible for storing all message attachments.
    pub const fn attachments(&self) -> Bucket {
        self.get_bucket("attachments")
    }

    pub const fn users(&self) -> Bucket {
        self.get_bucket("users")
    }

    /// Remove all S3 data for the given channel.
    ///
    /// ## Arguments
    ///
    /// * `channel` - The channel to remove all data for.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn remove_all_for_channel(&self, channel: impl Into<Snowflake<Channel>>) -> Result<(), AppError> {
        let bucket = self.attachments();
        let channel_id: Snowflake<Channel> = channel.into();
        let attachments = bucket.list_objects(channel_id.to_string(), None).await?;

        if attachments.is_empty() {
            return Ok(());
        }

        bucket
            .delete_objects(
                attachments
                    .into_iter()
                    .map(|o| o.key.unwrap_or_else(|| channel_id.to_string()))
                    .collect(),
            )
            .await
    }

    /// Remove all S3 data for the given guild.
    ///
    /// ## Arguments
    ///
    /// * `guild` - The guild to remove all data for.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn remove_all_for_guild(&self, guild: impl Into<Snowflake<Guild>>) -> Result<(), AppError> {
        let guild_id: i64 = guild.into().into();

        let channel_ids: Vec<i64> = sqlx::query!("SELECT id FROM channels WHERE guild_id = $1", guild_id)
            .fetch_all(self.app().db.pool())
            .await?
            .into_iter()
            .map(|r| r.id)
            .collect();

        for channel_id in channel_ids {
            self.remove_all_for_channel(channel_id).await?;
        }

        Ok(())
    }
}

/// An abstraction for S3 buckets.
#[derive(Clone, Debug)]
pub struct Bucket<'a> {
    name: &'static str,
    buckets: &'a Buckets,
}

impl<'a> Bucket<'a> {
    pub const fn new(buckets: &'a Buckets, name: &'static str) -> Self {
        Self { name, buckets }
    }

    /// The name of this bucket.
    pub const fn name(&self) -> &str {
        self.name
    }

    /// Fetch an object from this bucket.
    ///
    /// ## Arguments
    ///
    /// * `client` - The S3 client to use.
    /// * `key` - The key of the object to fetch.
    ///
    /// ## Returns
    ///
    /// [`Bytes`] - The object data.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn get_object(&self, key: impl Into<String>) -> Result<Bytes, AppError> {
        let mut resp = self
            .buckets
            .client()
            .get_object()
            .bucket(self.name)
            .key(key)
            .send()
            .await?;

        let mut bytes = BytesMut::new();
        while let Some(chunk) = resp.body.next().await {
            bytes.extend_from_slice(&chunk.expect("Failed to read S3 object chunk"));
        }

        Ok(bytes.freeze())
    }

    /// Upload an object to this bucket.
    ///
    /// ## Arguments
    ///
    /// * `client` - The S3 client to use.
    /// * `key` - The key of the object to upload.
    /// * `data` - The data to upload.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn put_object(
        &self,
        key: impl Into<String>,
        data: impl Into<ByteStream>,
        content_type: Mime,
    ) -> Result<(), AppError> {
        self.buckets
            .client()
            .put_object()
            .bucket(self.name)
            .content_type(content_type.to_string())
            .key(key)
            .body(data.into())
            .send()
            .await?;

        Ok(())
    }

    /// List objects in this bucket.
    ///
    /// ## Arguments
    ///
    /// * `client` - The S3 client to use.
    /// * `prefix` - The prefix to filter by.
    /// * `limit` - The maximum number of objects to fetch.
    ///
    /// ## Returns
    ///
    /// [`Vec<Object>`] - The objects fetched.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn list_objects(&self, prefix: impl Into<String>, limit: Option<i32>) -> Result<Vec<Object>, AppError> {
        let mut objects = Vec::new();

        // AWS-SDK has a nice pagination API to send continuation tokens implicitly, so we use that
        let mut req = self.buckets.client().list_objects_v2().bucket(self.name).prefix(prefix);

        if let Some(limit) = limit {
            req = req.max_keys(limit);
        }

        let mut paginator = req.into_paginator().send();

        while let Some(resp) = paginator.next().await {
            if let Some(contents) = resp?.contents {
                objects.extend(contents);
            }
        }

        Ok(objects)
    }

    /// Delete an object from this bucket.
    ///
    /// ## Arguments
    ///
    /// * `client` - The S3 client to use.
    /// * `key` - The key of the object to delete.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn delete_object(&self, key: impl Into<String>) -> Result<(), AppError> {
        self.buckets
            .client()
            .delete_object()
            .bucket(self.name)
            .key(key)
            .send()
            .await?;

        Ok(())
    }

    /// Delete multiple objects from this bucket.
    ///
    /// ## Arguments
    ///
    /// * `client` - The S3 client to use.
    /// * `keys` - The keys of the objects to delete.
    ///
    /// ## Errors
    ///
    /// * [`AppError::S3`] - If the S3 request fails.
    pub async fn delete_objects(&self, keys: Vec<impl Into<String>>) -> Result<(), AppError> {
        let objects: Vec<ObjectIdentifier> = keys
            .into_iter()
            .map(|k| {
                ObjectIdentifier::builder()
                    .set_key(Some(k.into()))
                    .build()
                    .expect("Failed to build ObjectIdentifier")
            })
            .collect();

        self.buckets
            .client()
            .delete_objects()
            .bucket(self.name)
            .delete(
                Delete::builder()
                    .set_objects(Some(objects))
                    .build()
                    .expect("Failed to build Delete"),
            )
            .send()
            .await?;

        Ok(())
    }
}
