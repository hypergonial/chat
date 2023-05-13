use lazy_static::lazy_static;
use sqlx::postgres::{PgPool, PgQueryResult};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedDatabase = Arc<RwLock<Database>>;

lazy_static! {
    pub static ref DB: SharedDatabase = SharedDatabase::default();
}

pub struct Database {
    pool: Option<PgPool>,
    is_connected: bool,
}

impl Database {
    /// Creates a new database instance
    ///
    /// Note: The database is not connected by default
    fn new() -> Self {
        Database {
            pool: None,
            is_connected: false,
        }
    }

    /// The database pool
    ///
    /// ## Panics
    ///
    /// Panics if the database is not connected
    pub fn pool(&self) -> &PgPool {
        self.pool
            .as_ref()
            .expect("Database is not connected or has been closed.")
    }

    /// Connects to the database
    pub async fn connect(&mut self, url: &str) -> Result<(), sqlx::Error> {
        self.pool = Some(PgPool::connect(url).await?);
        self.is_connected = true;
        self.create_schema().await?;
        Ok(())
    }

    /// Closes the database connection
    pub async fn close(&mut self) {
        self.pool().close().await;
        self.pool = None;
        self.is_connected = false;
    }

    /// Creates the database schema if it doesn't exist, otherwise does nothing
    async fn create_schema(&self) -> Result<PgQueryResult, sqlx::Error> {
        let query = include_str!("../../static/db/schema.sql");
        sqlx::query(query).execute(self.pool()).await?;
        sqlx::query("SELECT createSchema()")
            .execute(self.pool())
            .await
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}
