use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use my_budget_server::categories::validate_category_name;
use my_budget_server::models::Record;
use my_budget_server::records;
use my_budget_server::utils::get_user_database_from_pool;
use my_budget_server::{Db, DbPool};

use crate::constants::{SIMILAR_AMOUNT_RATIO, SIMILAR_RECORDS_DAYS, SIMILAR_RECORDS_LIMIT};
use crate::models::{CategoryInfo, SimilarRecord};

// ---------------------------------------------------------------------------
// Telegram user link
// ---------------------------------------------------------------------------

pub async fn upsert_telegram_link(
    db: &Db,
    telegram_user_id: i64,
    chat_id: i64,
    user_id: &str,
) -> Result<(), String> {
    let conn = db.write().await;
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
    let conn = db.read().await;
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

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

pub async fn load_categories(db_pool: &DbPool, user_id: &str) -> Result<Vec<CategoryInfo>, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name, is_income FROM categories ORDER BY name ASC",
            (),
        )
        .await
        .map_err(|_| "Failed to query categories".to_string())?;

    let mut categories = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query categories".to_string())?
    {
        let id: String = row.get(0).map_err(|_| "Invalid category".to_string())?;
        let name: String = row.get(1).map_err(|_| "Invalid category".to_string())?;
        let is_income: bool = row.get(2).map_err(|_| "Invalid category".to_string())?;
        categories.push(CategoryInfo {
            id,
            name,
            is_income,
        });
    }

    Ok(categories)
}

pub async fn get_or_create_category(
    db_pool: &DbPool,
    user_id: &str,
    name: &str,
    is_income: bool,
) -> Result<CategoryInfo, String> {
    let trimmed = name.trim();
    let fallback = if trimmed.is_empty() { "Other" } else { trimmed };
    validate_category_name(fallback).map_err(|(_, message)| message)?;

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.write().await;

    let mut existing = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE LOWER(name) = LOWER(?)",
            [fallback],
        )
        .await
        .map_err(|_| "Failed to query categories".to_string())?;

    if let Some(row) = existing
        .next()
        .await
        .map_err(|_| "Failed to query categories".to_string())?
    {
        let id: String = row.get(0).map_err(|_| "Invalid category".to_string())?;
        let name: String = row.get(1).map_err(|_| "Invalid category".to_string())?;
        let is_income: bool = row.get(2).map_err(|_| "Invalid category".to_string())?;
        return Ok(CategoryInfo {
            id,
            name,
            is_income,
        });
    }

    let category_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO categories (id, name, is_income) VALUES (?, ?, ?)",
        (category_id.as_str(), fallback, is_income),
    )
    .await
    .map_err(|_| "Failed to create category".to_string())?;

    Ok(CategoryInfo {
        id: category_id,
        name: fallback.to_string(),
        is_income,
    })
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

pub async fn load_similar_records(
    db_pool: &DbPool,
    user_id: &str,
    category_id: &str,
    amount: f64,
) -> Result<Vec<SimilarRecord>, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;
    let cutoff = OffsetDateTime::now_utc() - Duration::days(SIMILAR_RECORDS_DAYS);
    let cutoff_date = cutoff.date().to_string();
    let delta = amount.abs() * SIMILAR_AMOUNT_RATIO;
    let lower = amount - delta;
    let upper = amount + delta;

    let mut rows = conn
        .query(
            "SELECT name, amount FROM records \
             WHERE category_id = ? AND date >= ? AND amount BETWEEN ? AND ? \
             ORDER BY ABS(amount - ?) ASC, date DESC LIMIT ?",
            (
                category_id,
                cutoff_date.as_str(),
                lower,
                upper,
                amount,
                SIMILAR_RECORDS_LIMIT as i64,
            ),
        )
        .await
        .map_err(|_| "Failed to query similar records".to_string())?;

    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query similar records".to_string())?
    {
        let name: String = row.get(0).map_err(|_| "Invalid record".to_string())?;
        let amount: f64 = row.get(1).map_err(|_| "Invalid record".to_string())?;
        records.push(SimilarRecord { name, amount });
    }

    Ok(records)
}

pub async fn load_recent_records(
    db_pool: &DbPool,
    user_id: &str,
    limit: usize,
) -> Result<Vec<Record>, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;

    let mut rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records ORDER BY date DESC LIMIT ?",
            [limit as i64],
        )
        .await
        .map_err(|_| "Failed to query recent records".to_string())?;

    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query recent records".to_string())?
    {
        let record =
            records::extract_record_from_row(row).map_err(|(_, message)| message.to_string())?;
        records.push(record);
    }

    Ok(records)
}

pub async fn fetch_record_by_id(
    db_pool: &DbPool,
    user_id: &str,
    record_id: &str,
) -> Result<Record, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
            [record_id],
        )
        .await
        .map_err(|_| "Failed to query existing record".to_string())?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query existing record".to_string())?
    {
        records::extract_record_from_row(row).map_err(|(_, message)| message)
    } else {
        Err("Record not found.".to_string())
    }
}
