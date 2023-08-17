use sqlx::{migrate, postgres::PgPool};

pub struct Database {
    pool: Option<PgPool>,
    is_connected: bool,
}

impl Database {
    /// Creates a new database instance
    ///
    /// Note: The database is not connected by default
    pub const fn new() -> Self {
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
        migrate!("./migrations").run(self.pool()).await?;
        Ok(())
    }

    /// Closes the database connection
    pub async fn close(&mut self) {
        self.pool().close().await;
        self.pool = None;
        self.is_connected = false;
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}
