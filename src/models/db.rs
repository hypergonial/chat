use sqlx::{migrate, postgres::PgPool};

#[derive(Debug)]
pub struct Database {
    pool: Option<PgPool>,
}

impl Database {
    /// Creates a new database instance
    ///
    /// Note: The database is not connected by default
    pub const fn new() -> Self {
        Database { pool: None }
    }

    /// The database pool
    ///
    /// ## Panics
    ///
    /// If the database is not connected
    pub fn pool(&self) -> &PgPool {
        self.pool
            .as_ref()
            .expect("Database is not connected or has been closed.")
    }

    /// Checks if the database is connected
    ///
    /// ## Returns
    ///
    /// `true` if the database is connected, `false` otherwise
    pub fn is_connected(&self) -> bool {
        match self.pool {
            Some(ref pool) => !pool.is_closed(),
            None => false,
        }
    }

    /// Connects to the database
    ///
    /// ## Arguments
    ///
    /// * `url` - The postgres connection URL
    ///
    /// ## Errors
    ///
    /// * [`sqlx::Error`] - If the database connection fails
    pub async fn connect(&mut self, url: &str) -> Result<(), sqlx::Error> {
        self.pool = Some(PgPool::connect(url).await?);
        migrate!("./migrations").run(self.pool()).await?;
        Ok(())
    }

    /// Closes the database connection
    pub async fn close(&self) {
        self.pool().close().await;
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}
