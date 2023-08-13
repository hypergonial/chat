use std::net::SocketAddr;

use aws_sdk_s3::{
    config::{Credentials, Region},
    Client, Config as S3Config,
};
use dotenv::dotenv;
use lazy_static::lazy_static;
use tokio::sync::RwLock;

use super::db::Database;
use super::{bucket::Bucket, errors::ChatError, snowflake::Snowflake};
use crate::gateway::handler::Gateway;

lazy_static! {
    pub static ref APP: ApplicationState = ApplicationState::new();
}

/// Contains all the application state and manages application state changes.
pub struct ApplicationState {
    pub db: RwLock<Database>,
    pub gateway: RwLock<Gateway>,
    config: Config,
    s3: Client,
    buckets: Buckets,
}

impl ApplicationState {
    fn new() -> Self {
        let config = Config::from_env();
        let buckets = Buckets::new();

        let s3creds = Credentials::new(config.minio_access_key(), config.minio_secret_key(), None, None, "chat");

        let s3conf = S3Config::builder()
            .region(Region::new("vault"))
            .endpoint_url(config.minio_url())
            .credentials_provider(s3creds)
            .force_path_style(true) // MinIO does not support virtual hosts
            .build();

        ApplicationState {
            db: RwLock::new(Database::new()),
            config,
            gateway: RwLock::new(Gateway::new()),
            s3: Client::from_conf(s3conf),
            buckets,
        }
    }

    /// The application config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// All S3 buckets used by the application.
    pub fn buckets(&self) -> &Buckets {
        &self.buckets
    }

    /// The S3 SDK client.
    pub fn s3(&self) -> &Client {
        &self.s3
    }

    /// Initializes the application
    pub async fn init(&self) -> Result<(), sqlx::Error> {
        self.db.write().await.connect(self.config.database_url()).await
    }

    /// Closes the application and cleans up resources.
    pub async fn close(&self) {
        self.db.write().await.close().await
    }
}

/// Application configuration
pub struct Config {
    database_url: String,
    minio_url: String,
    minio_access_key: String,
    minio_secret_key: String,
    listen_addr: SocketAddr,
    machine_id: i32,
    process_id: i32,
}

impl Config {
    /// Creates a new config instance.
    pub const fn new(
        database_url: String,
        minio_url: String,
        minio_access_key: String,
        minio_secret_key: String,
        machine_id: i32,
        process_id: i32,
        listen_addr: SocketAddr,
    ) -> Self {
        Config {
            database_url,
            minio_url,
            minio_access_key,
            minio_secret_key,
            machine_id,
            process_id,
            listen_addr,
        }
    }

    /// The database url.
    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn minio_url(&self) -> &str {
        &self.minio_url
    }

    pub fn minio_access_key(&self) -> &str {
        &self.minio_access_key
    }

    pub fn minio_secret_key(&self) -> &str {
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

    /// Creates a new config from environment variables
    ///
    /// ## Panics
    ///
    /// Panics if any of the required environment variables are not set
    /// or if they are not in a valid format.
    pub fn from_env() -> Self {
        dotenv().ok();
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable must be set");
        let minio_url = std::env::var("MINIO_URL").expect("MINIO_URL environment variable must be set");
        let minio_access_key =
            std::env::var("MINIO_ACCESS_KEY").expect("MINIO_ACCESS_KEY environment variable must be set");
        let minio_secret_key =
            std::env::var("MINIO_SECRET_KEY").expect("MINIO_SECRET_KEY environment variable must be set");
        let machine_id = std::env::var("MACHINE_ID")
            .expect("MACHINE_ID environment variable must be set")
            .parse::<i32>()
            .expect("MACHINE_ID must be a valid integer");
        let process_id = std::env::var("PROCESS_ID")
            .expect("PROCESS_ID environment variable must be set")
            .parse::<i32>()
            .expect("PROCESS_ID must be a valid integer");
        let listen_addr = std::env::var("LISTEN_ADDR")
            .expect("LISTEN_ADDR environment variable must be set")
            .parse::<SocketAddr>()
            .expect("LISTEN_ADDR must be a valid socket address");
        Config::new(
            database_url,
            minio_url,
            minio_access_key,
            minio_secret_key,
            machine_id,
            process_id,
            listen_addr,
        )
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
    pub async fn remove_all_for_channel(&self, channel: impl Into<Snowflake>) -> Result<(), ChatError> {
        let bucket = APP.buckets().attachments();
        let channel_id: Snowflake = channel.into();
        let attachments = bucket.list_objects(APP.s3(), channel_id.to_string(), None).await?;
        bucket
            .delete_objects(
                APP.s3(),
                attachments
                    .into_iter()
                    .map(|o| o.key.unwrap_or(channel_id.to_string()))
                    .collect(),
            )
            .await?;
        Ok(())
    }

    /// Remove all S3 data for the given guild.
    ///
    /// ## Locks
    ///
    /// * `APP.db` (read)
    pub async fn remove_all_for_guild(&self, guild: impl Into<Snowflake>) -> Result<(), ChatError> {
        let guild_id: i64 = guild.into().into();
        let db = APP.db.read().await;

        let channel_ids: Vec<i64> = sqlx::query!("SELECT id FROM channels WHERE guild_id = $1", guild_id)
            .fetch_all(db.pool())
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
