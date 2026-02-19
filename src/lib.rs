pub mod auth;
pub mod categories;
pub mod config;
pub mod constants;
pub mod database;
pub mod db_pool;
pub mod friends;
pub mod models;
pub mod records;
pub mod splits;
pub mod utils;

// Re-export types at crate root for convenient importing
pub use crate::database::{Db, get_user_db, init_main_db};
pub use crate::db_pool::{DbPool, TransactionError, with_transaction};

/// Application state shared across all request handlers
#[derive(Clone)]
pub struct AppState {
    /// Main database for user authentication
    pub main_db: Db,
    /// Connection pool for user databases
    pub db_pool: DbPool,
}
