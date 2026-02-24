pub mod auth;
pub mod categories;
pub mod config;
pub mod constants;
pub mod database;
pub mod friends;
pub mod models;
pub mod records;
pub mod splits;
pub mod utils;

pub use crate::database::{Db, init_main_db};

use libsql::Connection;
use std::future::Future;
use std::pin::Pin;

/// Application state shared across all request handlers
#[derive(Clone)]
pub struct AppState {
    pub main_db: Db,
}

/// Errors that can occur during transaction management
#[derive(Debug)]
pub enum TransactionError {
    Begin,
    Commit,
}

/// Execute a function within a database transaction, returning handler-compatible errors.
pub async fn with_transaction<F, T, E>(db_conn: &Db, f: F) -> Result<T, E>
where
    F: for<'a> FnOnce(&'a Connection) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>,
    E: From<TransactionError>,
{
    let conn = db_conn.write().await;
    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|_| TransactionError::Begin)?;
    match f(&conn).await {
        Ok(result) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(|_| TransactionError::Commit)?;
            Ok(result)
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            Err(e)
        }
    }
}
