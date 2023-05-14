use lazy_static::lazy_static;
use tokio::sync::RwLock;

use super::db::Database;
use crate::gateway::handler::Gateway;
use dotenv::dotenv;

pub type SharedAppState = RwLock<ApplicationState>;

lazy_static! {
    pub static ref APP: SharedAppState = {
        let config = Config::from_env();
        let db = Database::new();
        let gateway = Gateway::new();
        RwLock::new(ApplicationState::new(db, gateway, config))
    };
}

/// Application configuration
pub struct Config {
    database_url: String,
    machine_id: i32,
    process_id: i32,
}

impl Config {
    /// Creates a new config instance.
    pub fn new(database_url: String, machine_id: i32, process_id: i32) -> Self {
        Config {
            database_url,
            machine_id,
            process_id,
        }
    }

    /// The database url.
    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    /// The machine id.
    pub fn machine_id(&self) -> i32 {
        self.machine_id
    }

    /// The process id.
    pub fn process_id(&self) -> i32 {
        self.process_id
    }

    /// Creates a new config from environment variables
    ///
    /// ## Panics
    ///
    /// Panics if any of the required environment variables are not set
    /// or if they are not in a valid format.
    pub fn from_env() -> Self {
        dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable must be set");
        let machine_id = std::env::var("MACHINE_ID")
            .expect("MACHINE_ID environment variable must be set")
            .parse::<i32>()
            .expect("MACHINE_ID must be a valid integer");
        let process_id = std::env::var("PROCESS_ID")
            .expect("PROCESS_ID environment variable must be set")
            .parse::<i32>()
            .expect("PROCESS_ID must be a valid integer");
        Config::new(database_url, machine_id, process_id)
    }
}

/// Contains all the application state and manages application state changes.
pub struct ApplicationState {
    pub db: Database,
    pub gateway: Gateway,
    config: Config,
}

impl ApplicationState {
    fn new(db: Database, gateway: Gateway, config: Config) -> Self {
        ApplicationState {
            db,
            gateway,
            config,
        }
    }

    /// The application config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Initializes the application
    pub async fn init(&mut self) -> Result<(), sqlx::Error> {
        self.db.connect(self.config.database_url()).await
    }
}
