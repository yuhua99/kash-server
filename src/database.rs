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
    id               TEXT    PRIMARY KEY,
    name             TEXT    NOT NULL,
    amount           REAL    NOT NULL,
    category_id      TEXT,
    date             TEXT    NOT NULL,
    pending          BOOLEAN NOT NULL DEFAULT 0,
    split_id         TEXT,
    settle           BOOLEAN NOT NULL DEFAULT 0,
    debtor_user_id   TEXT,
    creditor_user_id TEXT
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

const CREATE_FRIENDSHIP_RELATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS friendship_relations (
    id                TEXT    PRIMARY KEY,
    from_user_id      TEXT    NOT NULL,
    to_user_id        TEXT    NOT NULL,
    status            TEXT    NOT NULL,
    nickname          TEXT,
    requester_user_id TEXT    NOT NULL,
    requested_at      TEXT    NOT NULL,
    updated_at        TEXT    NOT NULL,
    UNIQUE(from_user_id, to_user_id)
);
"#;

const CREATE_FRIENDSHIP_FROM_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_from ON friendship_relations(from_user_id);
"#;

const CREATE_FRIENDSHIP_TO_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_to ON friendship_relations(to_user_id);
"#;

const CREATE_IDEMPOTENCY_KEYS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS idempotency_keys (
    key             TEXT    PRIMARY KEY,
    user_id         TEXT    NOT NULL,
    endpoint        TEXT    NOT NULL,
    payload_hash    TEXT    NOT NULL,
    response_status INTEGER NOT NULL,
    response_body   TEXT,
    created_at      TEXT    NOT NULL,
    expires_at      TEXT    NOT NULL
);
"#;

const CREATE_IDEMPOTENCY_USER_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_idempotency_user ON idempotency_keys(user_id);
"#;

pub type Db = Arc<RwLock<Connection>>;

/// Main users registry DB (users.db)
pub async fn init_main_db(data_dir: &str) -> Result<Db> {
    tokio::fs::create_dir_all(data_dir).await?;
    let path = Path::new(data_dir).join("users.db");
    let db = Builder::new_local(path).build().await?;
    let conn = db.connect()?;

    conn.execute(CREATE_USERS_TABLE, ()).await?;
    conn.execute(CREATE_TELEGRAM_USERS_TABLE, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_RELATIONS_TABLE, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_FROM_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_TO_INDEX, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_KEYS_TABLE, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_USER_INDEX, ()).await?;
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
