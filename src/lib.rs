pub mod auth;
pub mod categories;
pub mod config;
pub mod constants;
pub mod database;
pub mod db_pool;
pub mod models;
pub mod records;
pub mod utils;

// Re-export types at crate root for convenient importing
pub use crate::database::Db;
pub use crate::db_pool::{DbPool, TransactionError, with_transaction};

/// Application state shared across all request handlers
#[derive(Clone)]
pub struct AppState {
    /// Main database for user authentication
    pub main_db: Db,
    /// Connection pool for user databases
    pub db_pool: DbPool,
}
