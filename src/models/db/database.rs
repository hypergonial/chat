use std::sync::{Arc, Weak};

use sqlx::{migrate, postgres::PgPool};

use crate::models::state::ApplicationState;

#[derive(Clone, Debug)]
pub struct Database {
    pool: Option<PgPool>,
    app: Weak<ApplicationState>,
}

impl Database {
    /// Creates a new database instance
    ///
    /// Note: The database is not connected by default
    pub fn new() -> Self {
        Self {
            pool: None,
            app: Weak::new(),
        }
    }

    pub fn bind_to(&mut self, app: Weak<ApplicationState>) {
        self.app = app;
    }

    pub fn app(&self) -> Arc<ApplicationState> {
        self.app.upgrade().expect("Application state has been dropped.")
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
        self.pool.as_ref().map_or(false, |pool| !pool.is_closed())
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
