use aws_sdk_s3::{
    primitives::ByteStream,
    types::{Delete, Object, ObjectIdentifier},
    Client,
};
use bytes::{Bytes, BytesMut};
use mime::Mime;
use tokio_stream::StreamExt;

use super::errors::AppError;

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
    /// * [`AppError::S3Error`] - If the S3 request fails.
    pub async fn get_object(&self, client: &Client, key: impl Into<String>) -> Result<Bytes, AppError> {
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
    /// * [`AppError::S3Error`] - If the S3 request fails.
    pub async fn put_object(
        &self,
        client: &Client,
        key: impl Into<String>,
        data: impl Into<ByteStream>,
        content_type: Mime,
    ) -> Result<(), AppError> {
        client
            .put_object()
            .bucket(&self.name)
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
    /// * [`AppError::S3Error`] - If the S3 request fails.
    pub async fn list_objects(
        &self,
        client: &Client,
        prefix: impl Into<String>,
        limit: Option<i32>,
    ) -> Result<Vec<Object>, AppError> {
        let mut objects = Vec::new();

        // AWS-SDK has a nice pagination API to send continuation tokens implicitly, so we use that
        let mut req = client.list_objects_v2().bucket(&self.name).prefix(prefix);

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
    /// * [`AppError::S3Error`] - If the S3 request fails.
    pub async fn delete_object(&self, client: &Client, key: impl Into<String>) -> Result<(), AppError> {
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
    /// * [`AppError::S3Error`] - If the S3 request fails.
    pub async fn delete_objects(&self, client: &Client, keys: Vec<impl Into<String>>) -> Result<(), AppError> {
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
