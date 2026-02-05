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

const CREATE_TELEGRAM_USERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS telegram_users (
    telegram_user_id TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL,
    chat_id          TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id)
);
"#;

const CREATE_RECORDS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS records (
    id          TEXT    PRIMARY KEY,
    name        TEXT    NOT NULL,
    amount      REAL    NOT NULL,
    category_id TEXT    NOT NULL,
    date        TEXT    NOT NULL
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
CREATE INDEX IF NOT EXISTS idx_records_date ON records(date);
"#;

const CREATE_CATEGORIES_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_categories_name ON categories(name);
"#;

pub type Db = Arc<RwLock<Connection>>;

async fn ensure_records_date_column(conn: &Connection) -> Result<()> {
    let mut rows = conn.query("PRAGMA table_info(records)", ()).await?;
    let mut has_date = false;
    let mut has_timestamp = false;

    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        match name.as_str() {
            "date" => has_date = true,
            "timestamp" => has_timestamp = true,
            _ => {}
        }
    }

    if has_date {
        return Ok(());
    }

    conn.execute("ALTER TABLE records ADD COLUMN date TEXT", ())
        .await?;

    if has_timestamp {
        conn.execute(
            "UPDATE records SET date = strftime('%Y-%m-%d', timestamp, 'unixepoch') WHERE date IS NULL OR date = ''",
            (),
        )
        .await?;
    }

    Ok(())
}

/// Main users registry DB (users.db)
pub async fn init_main_db(data_dir: &str) -> Result<Db> {
    tokio::fs::create_dir_all(data_dir).await?;
    let path = Path::new(data_dir).join("users.db");
    let db = Builder::new_local(path).build().await?;
    let conn = db.connect()?;

    conn.execute(CREATE_USERS_TABLE, ()).await?;
    conn.execute(CREATE_TELEGRAM_USERS_TABLE, ()).await?;
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
    ensure_records_date_column(&conn).await?;
    conn.execute(CREATE_RECORDS_INDEX, ()).await?;
    conn.execute(CREATE_CATEGORIES_INDEX, ()).await?;

    Ok(Arc::new(RwLock::new(conn)))
}
