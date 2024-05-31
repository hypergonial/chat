use std::{net::SocketAddr, sync::Arc};

use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    config::{Credentials as S3Creds, Region},
    Client, Config as S3Config,
};

use derive_builder::Builder;
use dotenvy::dotenv;
use secrecy::{ExposeSecret, Secret};

use super::ops::Ops;
use crate::gateway::handler::Gateway;
use crate::models::{bucket::Buckets, db::Database, errors::BuildError};

pub type App = Arc<ApplicationState>;
pub type S3Client = Client;

/// Contains all the application state and manages application state changes.
#[derive(Clone)]
pub struct ApplicationState {
    pub db: Database,
    pub gateway: Gateway,
    pub config: Config,
    pub s3: Buckets,
}

impl ApplicationState {
    /// Create a new application state.
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database initialization fails.
    pub async fn new_shared() -> Result<Arc<Self>, sqlx::Error> {
        let config = Config::from_env();

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

        let buckets = Buckets::new(Client::from_conf(s3conf));

        let mut state = Self {
            db: Database::new(),
            config,
            gateway: Gateway::new(),
            s3: buckets,
        };

        state.init().await?;

        Ok(Arc::new_cyclic(|w| {
            state.db.bind_to(w.clone());
            state.gateway.bind_to(w.clone());
            state.s3.bind_to(w.clone());
            state
        }))
    }

    /// Initializes the application
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database connection fails.
    async fn init(&mut self) -> Result<(), sqlx::Error> {
        self.db.connect(self.config.database_url().expose_secret()).await
    }

    /// Closes the application and cleans up resources.
    pub async fn close(&self) {
        self.gateway.close();
        self.db.close().await;
    }

    #[inline]
    pub const fn ops(&self) -> Ops {
        Ops::new(self)
    }
}

/// Application configuration
#[derive(Debug, Clone, Builder)]
#[builder(setter(into), build_fn(error = "BuildError"))]
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
    pub const fn database_url(&self) -> &Secret<String> {
        &self.database_url
    }

    /// The URL for the `MinIO` server, an S3-compatible storage backend.
    pub const fn minio_url(&self) -> &String {
        &self.minio_url
    }

    /// The access key for S3.
    pub const fn minio_access_key(&self) -> &Secret<String> {
        &self.minio_access_key
    }

    /// The secret key for S3.
    pub const fn minio_secret_key(&self) -> &Secret<String> {
        &self.minio_secret_key
    }

    /// The machine id.
    pub const fn machine_id(&self) -> i32 {
        self.machine_id
    }

    /// The process id.
    pub const fn process_id(&self) -> i32 {
        self.process_id
    }

    /// The addres for the backend server to listen on.
    pub const fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// APP secret used to create JWT tokens.
    pub const fn app_secret(&self) -> &Secret<String> {
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
        Self::builder()
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
