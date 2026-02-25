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
    owner_user_id    TEXT    NOT NULL,
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
    id            TEXT    PRIMARY KEY,
    owner_user_id TEXT    NOT NULL,
    name          TEXT    NOT NULL,
    is_income     BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE(owner_user_id, name)
);
"#;

const CREATE_RECORDS_DATE_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_records_date ON records(date);
"#;

const CREATE_RECORDS_OWNER_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_records_owner ON records(owner_user_id);
"#;

const CREATE_CATEGORIES_OWNER_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_categories_owner ON categories(owner_user_id);
"#;

const CREATE_FRIENDSHIP_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS friendship (
    id                TEXT    PRIMARY KEY,
    from_user_id      TEXT    NOT NULL,
    to_user_id        TEXT    NOT NULL,
    pending           BOOLEAN NOT NULL DEFAULT 1,
    nickname          TEXT,
    requester_user_id TEXT    NOT NULL,
    UNIQUE(from_user_id, to_user_id)
);
"#;

const CREATE_FRIENDSHIP_FROM_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_from ON friendship(from_user_id);
"#;

const CREATE_FRIENDSHIP_TO_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_to ON friendship(to_user_id);
"#;

const CREATE_IDEMPOTENCY_KEYS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS idempotency_keys (
    id              TEXT    PRIMARY KEY,
    key             TEXT    NOT NULL,
    user_id         TEXT    NOT NULL,
    endpoint        TEXT    NOT NULL,
    payload_hash    TEXT    NOT NULL,
    response_status INTEGER NOT NULL,
    response_body   TEXT,
    created_at      TEXT    NOT NULL,
    expires_at      TEXT    NOT NULL,
    UNIQUE(user_id, endpoint, key)
);
"#;

const CREATE_IDEMPOTENCY_USER_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_idempotency_user ON idempotency_keys(user_id);
"#;

pub type Db = Arc<RwLock<Connection>>;

/// Single shared DB — contains all tables (users, records, categories, friends, etc.)
pub async fn init_main_db(data_dir: &str) -> Result<Db> {
    tokio::fs::create_dir_all(data_dir).await?;
    let path = Path::new(data_dir).join("users.db");
    let db = Builder::new_local(path).build().await?;
    let conn = db.connect()?;

    conn.execute(CREATE_USERS_TABLE, ()).await?;
    conn.execute(CREATE_TELEGRAM_USERS_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_TABLE, ()).await?;
    conn.execute(CREATE_CATEGORIES_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_DATE_INDEX, ()).await?;
    conn.execute(CREATE_RECORDS_OWNER_INDEX, ()).await?;
    conn.execute(CREATE_CATEGORIES_OWNER_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_TABLE, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_FROM_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_TO_INDEX, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_KEYS_TABLE, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_USER_INDEX, ()).await?;

    Ok(Arc::new(RwLock::new(conn)))
}
