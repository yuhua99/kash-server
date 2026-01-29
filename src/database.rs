use anyhow::Result;
use libsql::{Builder, Connection};
use std::{path::Path, sync::Arc};
use tokio::sync::RwLock;

const CREATE_USERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id             TEXT    PRIMARY KEY,
    name           TEXT    UNIQUE NOT NULL,
    password_hash  TEXT    NOT NULL
);
"#;

const CREATE_RECORDS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS records (
    id          TEXT    PRIMARY KEY,
    name        TEXT    NOT NULL,
    amount      REAL    NOT NULL,
    category_id TEXT    NOT NULL,
    timestamp   INTEGER NOT NULL
);
"#;

const CREATE_CATEGORIES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS categories (
    id        TEXT    PRIMARY KEY,
    name      TEXT    UNIQUE NOT NULL,
    is_income BOOLEAN NOT NULL DEFAULT FALSE
);
"#;

const CREATE_RECORDS_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_records_timestamp ON records(timestamp);
"#;

const CREATE_CATEGORIES_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_categories_name ON categories(name);
"#;

pub type Db = Arc<RwLock<Connection>>;

/// Main users registry DB (users.db)
pub async fn init_main_db(data_dir: &str) -> Result<Db> {
    tokio::fs::create_dir_all(data_dir).await?;
    let path = Path::new(data_dir).join("users.db");
    let db = Builder::new_local(path).build().await?;
    let conn = db.connect()?;

    conn.execute(CREATE_USERS_TABLE, ()).await?;

    Ok(Arc::new(RwLock::new(conn)))
}

/// Per-user isolated DB (user_{id}.db)
pub async fn get_user_db(data_dir: &str, user_id: &str) -> Result<Db> {
    let path = Path::new(data_dir).join(format!("user_{}.db", user_id));
    let db = Builder::new_local(path).build().await?;
    let conn = db.connect()?;

    // Create tables for user's expense data
    conn.execute(CREATE_RECORDS_TABLE, ()).await?;
    conn.execute(CREATE_CATEGORIES_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_INDEX, ()).await?;
    conn.execute(CREATE_CATEGORIES_INDEX, ()).await?;

    Ok(Arc::new(RwLock::new(conn)))
}
