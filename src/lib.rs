pub mod auth;
pub mod categories;
pub mod config;
pub mod constants;
pub mod database;
pub mod errors;
pub mod friends;
pub mod fx;
pub mod models;
pub mod money;
pub mod openapi;
pub mod records;
pub mod settings;
pub mod splits;
pub mod validation;

pub use crate::database::{Db, TransactionError, init_main_db, with_transaction};

/// Application state shared across all request handlers
#[derive(Clone)]
pub struct AppState {
    pub main_db: Db,
}
