use time::OffsetDateTime;

use kash_server::Db;

// ---------------------------------------------------------------------------
// Telegram user link
// ---------------------------------------------------------------------------

pub async fn upsert_telegram_link(
    db: &Db,
    telegram_user_id: i64,
    chat_id: i64,
    user_id: &str,
) -> Result<(), String> {
    let conn = db
        .connect()
        .map_err(|_| "Failed to connect to database".to_string())?;
    let created_at = OffsetDateTime::now_utc().unix_timestamp();

    conn.execute(
        "INSERT INTO telegram_users (telegram_user_id, user_id, chat_id, created_at) VALUES (?, ?, ?, ?)\
        ON CONFLICT(telegram_user_id) DO UPDATE SET user_id = excluded.user_id, chat_id = excluded.chat_id",
        (
            telegram_user_id.to_string(),
            user_id,
            chat_id.to_string(),
            created_at,
        ),
    )
    .await
    .map_err(|_| "Failed to link Telegram user".to_string())?;

    Ok(())
}

pub async fn fetch_linked_user_id(
    db: &Db,
    telegram_user_id: i64,
) -> Result<Option<String>, String> {
    let conn = db
        .connect()
        .map_err(|_| "Failed to connect to database".to_string())?;
    let mut rows = conn
        .query(
            "SELECT user_id FROM telegram_users WHERE telegram_user_id = ?",
            [telegram_user_id.to_string()],
        )
        .await
        .map_err(|_| "Failed to lookup Telegram user".to_string())?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to lookup Telegram user".to_string())?
    {
        let user_id: String = row
            .get(0)
            .map_err(|_| "Failed to read Telegram user".to_string())?;
        Ok(Some(user_id))
    } else {
        Ok(None)
    }
}
