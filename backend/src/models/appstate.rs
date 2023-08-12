use std::net::SocketAddr;

use dotenv::dotenv;
use lazy_static::lazy_static;
use s3::{creds::Credentials, region::Region, Bucket};
use tokio::sync::RwLock;

use super::db::Database;
use crate::gateway::handler::Gateway;

lazy_static! {
    pub static ref APP: ApplicationState = ApplicationState::new();
}

/// Contains all the application state and manages application state changes.
pub struct ApplicationState {
    pub db: RwLock<Database>,
    pub gateway: RwLock<Gateway>,
    config: Config,
    buckets: Buckets,
}

impl ApplicationState {
    fn new() -> Self {
        let config = Config::from_env();
        let buckets = Buckets::new(&config);

        ApplicationState {
            db: RwLock::new(Database::new()),
            config,
            gateway: RwLock::new(Gateway::new()),
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
    pub fn new(config: &Config) -> Self {
        let region = Region::Custom {
            region: "vault".to_string(),
            endpoint: config.minio_url().to_string(),
        };

        let credentials = Credentials::new(
            Some(config.minio_access_key()),
            Some(config.minio_secret_key()),
            None,
            None,
            None,
        )
        .unwrap();

        let attachments = Bucket::new("attachments", region, credentials).unwrap();
        Buckets { attachments }
    }

    /// The attachments bucket.
    /// It is responsible for storing all message attachments.
    pub fn attachments(&self) -> &Bucket {
        &self.attachments
    }
}
