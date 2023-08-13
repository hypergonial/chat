use aws_sdk_s3::{
    primitives::ByteStream,
    types::{Delete, Object, ObjectIdentifier},
    Client,
};
use bytes::{Bytes, BytesMut};
use mime::Mime;
use tokio_stream::StreamExt;

use super::errors::ChatError;

/// An abstraction for S3 buckets.
pub struct Bucket {
    name: String,
}

impl Bucket {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// The name of this bucket.
    pub fn name(&self) -> &str {
        &self.name
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
    /// * [`ChatError::S3Error`] - If the S3 request fails.
    pub async fn get_object(&self, client: &Client, key: impl Into<String>) -> Result<Bytes, ChatError> {
        let key = key.into();
        let mut resp = client.get_object().bucket(&self.name).key(key).send().await?;

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
    /// * [`ChatError::S3Error`] - If the S3 request fails.
    pub async fn put_object(
        &self,
        client: &Client,
        key: impl Into<String>,
        data: impl Into<ByteStream>,
        content_type: Mime,
    ) -> Result<(), ChatError> {
        let key = key.into();
        let data = data.into();

        client
            .put_object()
            .bucket(&self.name)
            .content_type(content_type.to_string())
            .key(key)
            .body(data)
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
    /// * `limit` - The maximum number of objects to fetch. Is capped at 1000.
    ///
    /// ## Returns
    ///
    /// [`Vec<Object>`] - The objects fetched.
    ///
    /// ## Errors
    ///
    /// * [`ChatError::S3Error`] - If the S3 request fails.
    pub async fn list_objects(&self, client: &Client, prefix: &str, limit: i32) -> Result<Vec<Object>, ChatError> {
        let mut objects = Vec::new();

        // AWS-SDK has a nice pagination API to send continuation tokens implicitly, so we use that
        let mut paginator = client
            .list_objects_v2()
            .bucket(&self.name)
            .prefix(prefix)
            .max_keys(limit)
            .into_paginator()
            .send();

        while let Some(resp) = paginator.next().await {
            let resp = resp?;

            if let Some(contents) = resp.contents {
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
    /// * [`ChatError::S3Error`] - If the S3 request fails.
    pub async fn delete_object(&self, client: &Client, key: impl Into<String>) -> Result<(), ChatError> {
        let key = key.into();

        client.delete_object().bucket(&self.name).key(key).send().await?;

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
    /// * [`ChatError::S3Error`] - If the S3 request fails.
    pub async fn delete_objects(&self, client: &Client, keys: Vec<impl Into<String>>) -> Result<(), ChatError> {
        let objects: Vec<ObjectIdentifier> = keys
            .into_iter()
            .map(|k| ObjectIdentifier::builder().set_key(Some(k.into())).build())
            .collect();

        client
            .delete_objects()
            .bucket(&self.name)
            .delete(Delete::builder().set_objects(Some(objects)).build())
            .send()
            .await?;

        Ok(())
    }
}
