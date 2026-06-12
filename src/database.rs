use anyhow::Result;
use libsql::{Builder, Connection, Database};
use std::{path::Path, sync::Arc};

const CREATE_USERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id             TEXT    PRIMARY KEY,
    name           TEXT    UNIQUE NOT NULL,
    password_hash  TEXT    NOT NULL,
    main_currency TEXT NOT NULL DEFAULT 'TWD'
);
"#;

const CREATE_RECORDS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS records (
    id               TEXT    PRIMARY KEY,
    owner_user_id    TEXT    NOT NULL REFERENCES users(id),
    name             TEXT    NOT NULL,
    amount           INTEGER NOT NULL,
    currency         TEXT    NOT NULL DEFAULT 'TWD',
    category_id      TEXT    REFERENCES categories(id),
    date             TEXT    NOT NULL CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    split_id         TEXT,
    settle           BOOLEAN NOT NULL DEFAULT 0 CHECK (settle IN (0, 1)),
    creditor_user_id TEXT    REFERENCES users(id)
);
"#;

const CREATE_CATEGORIES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS categories (
    id            TEXT    PRIMARY KEY,
    owner_user_id TEXT    NOT NULL REFERENCES users(id),
    name          TEXT    NOT NULL,
    is_income     BOOLEAN NOT NULL DEFAULT 0 CHECK (is_income IN (0, 1))
);
"#;

const CREATE_RECORDS_OWNER_DATE_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_records_owner_date
ON records(owner_user_id, date DESC, id DESC);
"#;

const CREATE_CATEGORIES_OWNER_LOWER_NAME_INDEX: &str = r#"
CREATE UNIQUE INDEX IF NOT EXISTS idx_categories_owner_lower_name
ON categories(owner_user_id, LOWER(name));
"#;

const CREATE_FRIENDSHIP_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS friendship (
    id                TEXT    PRIMARY KEY,
    from_user_id      TEXT    NOT NULL REFERENCES users(id),
    to_user_id        TEXT    NOT NULL REFERENCES users(id),
    pending           BOOLEAN NOT NULL DEFAULT 1 CHECK (pending IN (0, 1)),
    nickname          TEXT,
    requester_user_id TEXT    NOT NULL REFERENCES users(id),
    UNIQUE(from_user_id, to_user_id),
    CHECK (from_user_id != to_user_id)
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
    user_id         TEXT    NOT NULL REFERENCES users(id),
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

const CREATE_EXCHANGE_RATES_DAILY_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS exchange_rates_daily (
    date                TEXT NOT NULL,
    currency            TEXT NOT NULL,
    rate                REAL NOT NULL CHECK (rate > 0),
    PRIMARY KEY (date, currency)
);
"#;

const CREATE_EXCHANGE_RATES_CURRENCY_DATE_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_exchange_rates_currency_date
ON exchange_rates_daily(currency, date DESC);
"#;

pub type Db = Arc<Database>;

pub async fn db_conn(db: &Db) -> Result<Connection, libsql::Error> {
    let conn = db.connect()?;
    let mut foreign_key_rows = conn.query("PRAGMA foreign_keys = ON", ()).await?;
    while foreign_key_rows.next().await?.is_some() {}
    Ok(conn)
}

/// Single shared DB — contains all tables (users, records, categories, friends, etc.)
pub async fn init_main_db(data_dir: &str) -> Result<Db> {
    tokio::fs::create_dir_all(data_dir).await?;
    let path = Path::new(data_dir).join("users.db");
    let db = Arc::new(Builder::new_local(path).build().await?);
    let conn = db_conn(&db).await?;

    let mut journal_rows = conn.query("PRAGMA journal_mode = WAL", ()).await?;
    while journal_rows.next().await?.is_some() {}
    let mut timeout_rows = conn.query("PRAGMA busy_timeout = 5000", ()).await?;
    while timeout_rows.next().await?.is_some() {}

    conn.execute(CREATE_USERS_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_TABLE, ()).await?;
    conn.execute(CREATE_CATEGORIES_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_OWNER_DATE_INDEX, ()).await?;
    conn.execute(CREATE_CATEGORIES_OWNER_LOWER_NAME_INDEX, ())
        .await?;
    conn.execute(CREATE_FRIENDSHIP_TABLE, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_FROM_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_TO_INDEX, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_KEYS_TABLE, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_USER_INDEX, ()).await?;
    conn.execute(CREATE_EXCHANGE_RATES_DAILY_TABLE, ()).await?;
    conn.execute(CREATE_EXCHANGE_RATES_CURRENCY_DATE_INDEX, ())
        .await?;

    Ok(db)
}
