use std::{net::SocketAddr, sync::OnceLock};

use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    config::{Credentials as S3Creds, Region},
    Client, Config as S3Config,
};
use derive_builder::Builder;
use dotenvy::dotenv;
use secrecy::{ExposeSecret, Secret};

use super::db::Database;
use super::{
    bucket::Bucket,
    errors::{AppError, BuilderError},
    snowflake::Snowflake,
};
use crate::gateway::handler::Gateway;

pub static APP: OnceLock<ApplicationState> = OnceLock::new();

pub fn app() -> &'static ApplicationState {
    APP.get().expect("ApplicationState was not initialized")
}

/// Contains all the application state and manages application state changes.
pub struct ApplicationState {
    pub db: Database,
    pub gateway: Gateway,
    pub config: Config,
    pub s3: Client,
    pub buckets: Buckets,
}

impl ApplicationState {
    pub fn new() -> Self {
        let config = Config::from_env();
        let buckets = Buckets::new();

        let s3creds = S3Creds::new(
            config.minio_access_key().expose_secret(),
            config.minio_secret_key().expose_secret(),
            None,
            None,
            "chat",
        );

        let s3conf = S3Config::builder()
            .region(Region::new("vault"))
            .endpoint_url(config.minio_url())
            .credentials_provider(s3creds)
            .force_path_style(true) // MinIO does not support virtual hosts
            .behavior_version(BehaviorVersion::v2024_03_28())
            .build();

        ApplicationState {
            db: Database::new(),
            config,
            gateway: Gateway::new(),
            s3: Client::from_conf(s3conf),
            buckets,
        }
    }

    /// Initializes the application
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database connection fails.
    pub async fn init(&mut self) -> Result<(), sqlx::Error> {
        self.db.connect(self.config.database_url().expose_secret()).await
    }

    /// Closes the application and cleans up resources.
    pub async fn close(&self) {
        self.db.close().await
    }
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Application configuration
#[derive(Debug, Builder)]
#[builder(setter(into), build_fn(error = "BuilderError"))]
pub struct Config {
    database_url: Secret<String>,
    minio_url: String,
    minio_access_key: Secret<String>,
    minio_secret_key: Secret<String>,
    listen_addr: SocketAddr,
    machine_id: i32,
    process_id: i32,
    app_secret: Secret<String>,
}

impl Config {
    /// Create a new builder to construct a [`Config`].
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// The database URL.
    pub fn database_url(&self) -> &Secret<String> {
        &self.database_url
    }

    /// The URL for the MinIO server, an S3-compatible storage backend.
    pub fn minio_url(&self) -> &str {
        &self.minio_url
    }

    /// The access key for S3.
    pub fn minio_access_key(&self) -> &Secret<String> {
        &self.minio_access_key
    }

    /// The secret key for S3.
    pub fn minio_secret_key(&self) -> &Secret<String> {
        &self.minio_secret_key
    }

    /// The machine id.
    pub fn machine_id(&self) -> i32 {
        self.machine_id
    }

    /// The process id.
    pub fn process_id(&self) -> i32 {
        self.process_id
    }

    /// The addres for the backend server to listen on.
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// APP secret used to create JWT tokens.
    pub fn app_secret(&self) -> &Secret<String> {
        &self.app_secret
    }

    /// Creates a new config from environment variables
    ///
    /// ## Panics
    ///
    /// Panics if any of the required environment variables are not set
    /// or if they are not in a valid format.
    pub fn from_env() -> Self {
        dotenv().ok();
        Config::builder()
            .database_url(std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable must be set"))
            .minio_url(std::env::var("MINIO_URL").expect("MINIO_URL environment variable must be set"))
            .minio_access_key(
                std::env::var("MINIO_ACCESS_KEY").expect("MINIO_ACCESS_KEY environment variable must be set"),
            )
            .minio_secret_key(
                std::env::var("MINIO_SECRET_KEY").expect("MINIO_SECRET_KEY environment variable must be set"),
            )
            .machine_id(
                std::env::var("MACHINE_ID")
                    .expect("MACHINE_ID environment variable must be set")
                    .parse::<i32>()
                    .expect("MACHINE_ID must be a valid integer"),
            )
            .process_id(
                std::env::var("PROCESS_ID")
                    .expect("PROCESS_ID environment variable must be set")
                    .parse::<i32>()
                    .expect("PROCESS_ID must be a valid integer"),
            )
            .listen_addr(
                std::env::var("LISTEN_ADDR")
                    .expect("LISTEN_ADDR environment variable must be set")
                    .parse::<SocketAddr>()
                    .expect("LISTEN_ADDR must be a valid socket address"),
            )
            .app_secret(std::env::var("APP_SECRET").expect("APP_SECRET environment variable must be set"))
            .build()
            .expect("Failed to create application configuration.")
    }
}

/// All S3 buckets used by the application.
pub struct Buckets {
    attachments: Bucket,
}

impl Buckets {
    /// Create all buckets from the given config.
    pub fn new() -> Self {
        let attachments = Bucket::new("attachments");
        Buckets { attachments }
    }

    /// The attachments bucket.
    /// It is responsible for storing all message attachments.
    pub fn attachments(&self) -> &Bucket {
        &self.attachments
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
    pub async fn remove_all_for_channel(&self, channel: impl Into<Snowflake>) -> Result<(), AppError> {
        let bucket = app().buckets.attachments();
        let channel_id: Snowflake = channel.into();
        let attachments = bucket.list_objects(&app().s3, channel_id.to_string(), None).await?;

        if attachments.is_empty() {
            return Ok(());
        }

        bucket
            .delete_objects(
                &app().s3,
                attachments
                    .into_iter()
                    .map(|o| o.key.unwrap_or(channel_id.to_string()))
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
    pub async fn remove_all_for_guild(&self, guild: impl Into<Snowflake>) -> Result<(), AppError> {
        let guild_id: i64 = guild.into().into();

        let channel_ids: Vec<i64> = sqlx::query!("SELECT id FROM channels WHERE guild_id = $1", guild_id)
            .fetch_all(app().db.pool())
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

impl Default for Buckets {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Buckets> for Vec<Bucket> {
    fn from(buckets: Buckets) -> Self {
        vec![buckets.attachments]
    }
}
