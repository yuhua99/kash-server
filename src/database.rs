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
    id            TEXT    PRIMARY KEY,
    owner_user_id TEXT    NOT NULL REFERENCES users(id),
    name          TEXT    NOT NULL,
    amount        INTEGER NOT NULL,
    currency      TEXT    NOT NULL DEFAULT 'TWD',
    category_id   TEXT    REFERENCES categories(id),
    date          TEXT    NOT NULL CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
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

const CREATE_SPLITS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS splits (
    id               TEXT    PRIMARY KEY,
    creditor_user_id TEXT    NOT NULL REFERENCES users(id),
    description      TEXT    NOT NULL,
    currency         TEXT    NOT NULL,
    date             TEXT    NOT NULL CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    total_amount     INTEGER NOT NULL CHECK (total_amount > 0),
    created_at       TEXT    NOT NULL
);
"#;

const CREATE_SPLIT_PARTICIPANTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS split_participants (
    id                  TEXT    PRIMARY KEY,
    split_id            TEXT    NOT NULL REFERENCES splits(id) ON DELETE CASCADE,
    debtor_user_id      TEXT    NOT NULL REFERENCES users(id),
    amount              INTEGER NOT NULL CHECK (amount > 0),
    settled             BOOLEAN NOT NULL DEFAULT 0 CHECK (settled IN (0, 1)),
    finalized_record_id TEXT    UNIQUE REFERENCES records(id) ON DELETE SET NULL,
    UNIQUE (split_id, debtor_user_id)
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

const CREATE_SPLIT_PARTICIPANTS_DEBTOR_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_split_participants_debtor
ON split_participants(debtor_user_id, settled);
"#;

const CREATE_SPLIT_PARTICIPANTS_SPLIT_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_split_participants_split
ON split_participants(split_id);
"#;

const CREATE_SPLITS_CREDITOR_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_splits_creditor ON splits(creditor_user_id);
"#;

const CREATE_FRIENDSHIP_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS friendship (
    id                TEXT    PRIMARY KEY,
    user_low_id       TEXT    NOT NULL REFERENCES users(id),
    user_high_id      TEXT    NOT NULL REFERENCES users(id),
    requester_user_id TEXT    NOT NULL REFERENCES users(id),
    pending           BOOLEAN NOT NULL DEFAULT 1 CHECK (pending IN (0, 1)),
    CHECK (user_low_id < user_high_id),
    CHECK (requester_user_id IN (user_low_id, user_high_id)),
    UNIQUE (user_low_id, user_high_id)
);
"#;

const CREATE_FRIENDSHIP_NICKNAMES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS friendship_nicknames (
    friendship_id TEXT NOT NULL REFERENCES friendship(id) ON DELETE CASCADE,
    owner_user_id TEXT NOT NULL REFERENCES users(id),
    nickname      TEXT NOT NULL,
    PRIMARY KEY (friendship_id, owner_user_id)
);
"#;

const CREATE_FRIENDSHIP_LOW_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_from ON friendship(user_low_id);
"#;

const CREATE_FRIENDSHIP_HIGH_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_to ON friendship(user_high_id);
"#;

const CREATE_FRIENDSHIP_NICKNAMES_OWNER_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_friendship_nicknames_owner ON friendship_nicknames(owner_user_id);
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

/// Errors that can occur during transaction management
#[derive(Debug)]
pub enum TransactionError {
    Begin,
    Commit,
}

/// Execute a function within a database transaction, returning handler-compatible errors.
pub async fn with_transaction<F, T, E>(db: &Db, f: F) -> Result<T, E>
where
    F: for<'a> FnOnce(
        &'a Connection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'a>,
    >,
    E: From<TransactionError>,
{
    let conn = db_conn(db).await.map_err(|_| TransactionError::Begin)?;
    conn.execute("BEGIN IMMEDIATE", ())
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

pub async fn db_conn(db: &Db) -> Result<Connection, libsql::Error> {
    let conn = db.connect()?;
    let mut foreign_key_rows = conn.query("PRAGMA foreign_keys = ON", ()).await?;
    while foreign_key_rows.next().await?.is_some() {}
    let mut timeout_rows = conn.query("PRAGMA busy_timeout = 5000", ()).await?;
    while timeout_rows.next().await?.is_some() {}
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

    conn.execute(CREATE_USERS_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_TABLE, ()).await?;
    conn.execute(CREATE_CATEGORIES_TABLE, ()).await?;
    conn.execute(CREATE_SPLITS_TABLE, ()).await?;
    conn.execute(CREATE_SPLIT_PARTICIPANTS_TABLE, ()).await?;
    conn.execute(CREATE_RECORDS_OWNER_DATE_INDEX, ()).await?;
    conn.execute(CREATE_CATEGORIES_OWNER_LOWER_NAME_INDEX, ())
        .await?;
    conn.execute(CREATE_SPLIT_PARTICIPANTS_DEBTOR_INDEX, ())
        .await?;
    conn.execute(CREATE_SPLIT_PARTICIPANTS_SPLIT_INDEX, ())
        .await?;
    conn.execute(CREATE_SPLITS_CREDITOR_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_TABLE, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_NICKNAMES_TABLE, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_LOW_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_HIGH_INDEX, ()).await?;
    conn.execute(CREATE_FRIENDSHIP_NICKNAMES_OWNER_INDEX, ())
        .await?;
    conn.execute(CREATE_IDEMPOTENCY_KEYS_TABLE, ()).await?;
    conn.execute(CREATE_IDEMPOTENCY_USER_INDEX, ()).await?;
    conn.execute(CREATE_EXCHANGE_RATES_DAILY_TABLE, ()).await?;
    conn.execute(CREATE_EXCHANGE_RATES_CURRENCY_DATE_INDEX, ())
        .await?;

    Ok(db)
}
