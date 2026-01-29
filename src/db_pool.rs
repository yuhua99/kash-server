use anyhow::Result;
use libsql::Connection;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Type alias for a shared database connection
pub type DbConnection = Arc<RwLock<Connection>>;

/// Default maximum number of connections to cache
const DEFAULT_MAX_CONNECTIONS: usize = 100;

/// Entry in the connection pool with last-access tracking for LRU eviction
struct PoolEntry {
    connection: DbConnection,
    last_accessed: Instant,
}

/// Database connection pool that caches user database connections
/// with LRU eviction when the pool exceeds max_connections
#[derive(Clone)]
pub struct DbPool {
    /// Path to the data directory containing database files
    data_path: String,
    /// Maximum number of connections to cache
    max_connections: usize,
    /// Cache of user database connections, keyed by user_id
    /// Using Arc<RwLock<HashMap>> allows multiple readers or one writer
    connections: Arc<RwLock<HashMap<String, PoolEntry>>>,
}

impl DbPool {
    /// Create a new database pool with default max connections
    pub fn new(data_path: String) -> Self {
        Self::with_max_connections(data_path, DEFAULT_MAX_CONNECTIONS)
    }

    /// Create a new database pool with a custom max connections limit
    pub fn with_max_connections(data_path: String, max_connections: usize) -> Self {
        Self {
            data_path,
            max_connections,
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a database connection for a specific user
    /// If the connection exists in the pool, it's reused
    /// If not, a new connection is created and cached
    /// When the pool is full, the least recently used connection is evicted
    pub async fn get_user_db(&self, user_id: &str) -> Result<DbConnection> {
        // First, try to get from cache with a read lock (allows concurrent reads)
        {
            let mut connections = self.connections.write().await;
            if let Some(entry) = connections.get_mut(user_id) {
                entry.last_accessed = Instant::now();
                return Ok(entry.connection.clone());
            }
        }

        // Connection not in cache, acquire write lock to create it
        let mut connections = self.connections.write().await;

        // Double-check: another thread might have created it while we waited for the write lock
        if let Some(entry) = connections.get_mut(user_id) {
            entry.last_accessed = Instant::now();
            return Ok(entry.connection.clone());
        }

        // Evict LRU connection if at capacity
        if connections.len() >= self.max_connections {
            self.evict_lru(&mut connections);
        }

        // Create new connection and add to cache
        let conn = crate::database::get_user_db(&self.data_path, user_id).await?;
        connections.insert(
            user_id.to_string(),
            PoolEntry {
                connection: conn.clone(),
                last_accessed: Instant::now(),
            },
        );

        Ok(conn)
    }

    /// Evict the least recently used connection from the pool
    fn evict_lru(&self, connections: &mut HashMap<String, PoolEntry>) {
        if let Some((oldest_key, _)) = connections
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
        {
            let key_to_remove = oldest_key.clone();
            connections.remove(&key_to_remove);
        }
    }

    /// Get the number of cached connections (useful for monitoring)
    pub async fn pool_size(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Clear all cached connections (useful for testing or maintenance)
    #[allow(dead_code)]
    pub async fn clear(&self) {
        self.connections.write().await.clear();
    }

    /// Execute a function within a database transaction
    /// This ensures atomicity - either all operations succeed or none do
    ///
    /// Example usage:
    /// ```ignore
    /// pool.with_transaction(&user_db, |conn| async move {
    ///     conn.execute("INSERT INTO ...", ()).await?;
    ///     conn.execute("UPDATE ...", ()).await?;
    ///     Ok(())
    /// }).await?;
    /// ```
    pub async fn with_transaction<F, Fut, T>(&self, db_conn: &DbConnection, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Acquire write lock for exclusive access during transaction
        let conn = db_conn.write().await;

        // Begin transaction
        conn.execute("BEGIN TRANSACTION", ()).await?;

        // Execute the provided function
        match f(&*conn).await {
            Ok(result) => {
                // Success - commit transaction
                conn.execute("COMMIT", ()).await?;
                Ok(result)
            }
            Err(e) => {
                // Error - rollback transaction
                let _ = conn.execute("ROLLBACK", ()).await;
                Err(e)
            }
        }
    }
}

/// Execute a function within a database transaction, returning handler-compatible errors
/// This is a standalone function that works with any DbConnection
///
/// The closure must return a boxed future to handle lifetime issues with async closures.
pub async fn with_transaction<F, T, E>(db_conn: &DbConnection, f: F) -> Result<T, E>
where
    F: for<'a> FnOnce(&'a Connection) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>,
    E: From<TransactionError>,
{
    // Acquire write lock for exclusive access during transaction
    let conn = db_conn.write().await;

    // Begin transaction
    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|_| TransactionError::Begin)?;

    // Execute the provided function
    match f(&*conn).await {
        Ok(result) => {
            // Success - commit transaction
            conn.execute("COMMIT", ())
                .await
                .map_err(|_| TransactionError::Commit)?;
            Ok(result)
        }
        Err(e) => {
            // Error - rollback transaction
            let _ = conn.execute("ROLLBACK", ()).await;
            Err(e)
        }
    }
}

/// Errors that can occur during transaction management
#[derive(Debug)]
pub enum TransactionError {
    Begin,
    Commit,
}
