use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use my_budget_server::categories::validate_category_name;
use my_budget_server::models::{CreateRecordPayload, Record};
use my_budget_server::records;
use my_budget_server::utils::{
    get_user_database_from_pool, validate_date, validate_offset, validate_records_limit,
};
use my_budget_server::{Db, DbPool};

use crate::helpers::{normalize_amount_by_category, resolve_category_id};
use crate::models::{BotState, CategoryInfo};

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
// Category helpers
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
// AI tool execution
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateRecordToolInput {
    name: String,
    amount: f64,
    category_id: Option<String>,
    category_name: Option<String>,
    date: Option<String>,
    is_income: Option<bool>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct EditRecordToolInput {
    record_id: Option<String>,
    record_name: Option<String>,
    name: Option<String>,
    amount: Option<f64>,
    category_id: Option<String>,
    category_name: Option<String>,
    date: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ListRecordsToolInput {
    start_date: Option<String>,
    end_date: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    category_id: Option<String>,
    category_name: Option<String>,
    name_contains: Option<String>,
    min_amount: Option<f64>,
    max_amount: Option<f64>,
}

pub async fn execute_tool_call(
    state: &BotState,
    user_id: &str,
    tool_name: &str,
    arguments: &str,
) -> Result<serde_json::Value, String> {
    match tool_name {
        "create_record" => {
            let input: CreateRecordToolInput = parse_tool_arguments(arguments)?;
            create_record_tool(&state.db_pool, user_id, input).await
        }
        "edit_record" => {
            let input: EditRecordToolInput = parse_tool_arguments(arguments)?;
            edit_record_tool(&state.db_pool, user_id, input).await
        }
        "list_records" => {
            let input: ListRecordsToolInput = parse_tool_arguments(arguments)?;
            list_records_tool(&state.db_pool, user_id, input).await
        }
        _ => Err(format!("Unknown tool: {tool_name}")),
    }
}

fn parse_tool_arguments<T: for<'de> Deserialize<'de>>(arguments: &str) -> Result<T, String> {
    serde_json::from_str(arguments).map_err(|_| "Tool arguments are invalid JSON".to_string())
}

async fn create_record_tool(
    db_pool: &DbPool,
    user_id: &str,
    input: CreateRecordToolInput,
) -> Result<serde_json::Value, String> {
    let categories = load_categories(db_pool, user_id).await?;
    let category = resolve_or_create_category(
        db_pool,
        user_id,
        &categories,
        input.category_id.as_deref(),
        input.category_name.as_deref(),
        input.is_income,
    )
    .await?;

    let date = match input
        .date
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => OffsetDateTime::now_utc().date().to_string(),
    };
    validate_date(&date).map_err(|(_, message)| message)?;

    let payload = CreateRecordPayload {
        name: input.name.trim().to_string(),
        amount: input.amount,
        category_id: category.id.clone(),
        date,
    };

    let record = records::create_record_for_user(db_pool, user_id, payload)
        .await
        .map_err(|(_, message)| message)?;

    Ok(json!({
        "ok": true,
        "record": {
            "id": record.id,
            "name": record.name,
            "amount": record.amount,
            "category_id": record.category_id,
            "category_name": category.name,
            "date": record.date,
        }
    }))
}

async fn edit_record_tool(
    db_pool: &DbPool,
    user_id: &str,
    input: EditRecordToolInput,
) -> Result<serde_json::Value, String> {
    let categories = load_categories(db_pool, user_id).await?;

    let existing = if let Some(record_id) = input
        .record_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        fetch_record_by_id(db_pool, user_id, record_id).await?
    } else if let Some(record_name) = input
        .record_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        fetch_record_by_exact_name(db_pool, user_id, record_name).await?
    } else {
        return Err("edit_record requires record_id or record_name".to_string());
    };

    let new_name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(name) = &new_name {
        records::validate_record_name(name).map_err(|(_, message)| message)?;
    }

    if let Some(amount) = input.amount {
        records::validate_record_amount(amount).map_err(|(_, message)| message)?;
    }

    let new_date = input
        .date
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(date) = &new_date {
        validate_date(date).map_err(|(_, message)| message)?;
    }

    let provided_category_id = input.category_id.as_deref().unwrap_or("");
    let provided_category_name = input.category_name.as_deref().unwrap_or("");
    let new_category_id =
        if provided_category_id.trim().is_empty() && provided_category_name.trim().is_empty() {
            None
        } else {
            resolve_category_id(&categories, provided_category_id, provided_category_name).or_else(
                || {
                    if provided_category_name.trim().is_empty() {
                        None
                    } else {
                        categories
                            .iter()
                            .find(|category| category.id == provided_category_id)
                            .map(|category| category.id.clone())
                    }
                },
            )
        };

    if (!provided_category_id.trim().is_empty() || !provided_category_name.trim().is_empty())
        && new_category_id.is_none()
    {
        return Err(format!(
            "Category not found. Available categories: {}",
            format_category_options(&categories)
        ));
    }

    if new_name.is_none()
        && input.amount.is_none()
        && new_category_id.is_none()
        && new_date.is_none()
    {
        return Err("edit_record needs at least one field to update".to_string());
    }

    let updated_name = new_name.unwrap_or_else(|| existing.name.clone());
    let updated_category_id = new_category_id.unwrap_or_else(|| existing.category_id.clone());
    let updated_date = new_date.unwrap_or_else(|| existing.date.clone());

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.write().await;

    let updated_amount = if let Some(amount) = input.amount {
        let is_income = get_category_is_income(&conn, &updated_category_id).await?;
        normalize_amount_by_category(amount, is_income)
    } else {
        existing.amount
    };

    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ?",
            (
                updated_name.as_str(),
                updated_amount,
                updated_category_id.as_str(),
                updated_date.as_str(),
                existing.id.as_str(),
            ),
        )
        .await
        .map_err(|_| "Failed to update record".to_string())?;
    if affected_rows == 0 {
        return Err("Record not found or no changes made".to_string());
    }

    let category_name = categories
        .iter()
        .find(|category| category.id == updated_category_id)
        .map(|category| category.name.as_str())
        .unwrap_or("Unknown")
        .to_string();

    Ok(json!({
        "ok": true,
        "record": {
            "id": existing.id,
            "name": updated_name,
            "amount": updated_amount,
            "category_id": updated_category_id,
            "category_name": category_name,
            "date": updated_date,
        }
    }))
}

async fn list_records_tool(
    db_pool: &DbPool,
    user_id: &str,
    input: ListRecordsToolInput,
) -> Result<serde_json::Value, String> {
    if let Some(start_date) = &input.start_date {
        validate_date(start_date).map_err(|(_, message)| message)?;
    }
    if let Some(end_date) = &input.end_date {
        validate_date(end_date).map_err(|(_, message)| message)?;
    }

    let start_date = input.start_date.unwrap_or_else(|| "0000-01-01".to_string());
    let end_date = input.end_date.unwrap_or_else(|| "9999-12-31".to_string());

    let limit = validate_records_limit(input.limit).map_err(|(_, message)| message)?;
    let offset = validate_offset(input.offset).map_err(|(_, message)| message)?;

    if let (Some(min), Some(max)) = (input.min_amount, input.max_amount)
        && min > max
    {
        return Err("min_amount cannot be greater than max_amount".to_string());
    }
    let min_amount = input.min_amount.unwrap_or(-1.0e15);
    let max_amount = input.max_amount.unwrap_or(1.0e15);

    let categories = load_categories(db_pool, user_id).await?;
    let category_filter = resolve_category_filter_id(
        &categories,
        input.category_id.as_deref(),
        input.category_name.as_deref(),
    )?;

    let name_filter = input
        .name_contains
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_default();

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*) FROM records \
             WHERE date BETWEEN ? AND ? \
             AND (? = '' OR category_id = ?) \
             AND (? = '' OR INSTR(LOWER(name), LOWER(?)) > 0) \
             AND amount BETWEEN ? AND ?",
            (
                start_date.as_str(),
                end_date.as_str(),
                category_filter.as_str(),
                category_filter.as_str(),
                name_filter.as_str(),
                name_filter.as_str(),
                min_amount,
                max_amount,
            ),
        )
        .await
        .map_err(|_| "Failed to count records".to_string())?;

    let total_count: u32 = if let Some(row) = count_rows
        .next()
        .await
        .map_err(|_| "Failed to count records".to_string())?
    {
        row.get(0).map_err(|_| "Invalid record count".to_string())?
    } else {
        0
    };

    let mut rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records \
             WHERE date BETWEEN ? AND ? \
             AND (? = '' OR category_id = ?) \
             AND (? = '' OR INSTR(LOWER(name), LOWER(?)) > 0) \
             AND amount BETWEEN ? AND ? \
             ORDER BY date DESC, id DESC LIMIT ? OFFSET ?",
            (
                start_date.as_str(),
                end_date.as_str(),
                category_filter.as_str(),
                category_filter.as_str(),
                name_filter.as_str(),
                name_filter.as_str(),
                min_amount,
                max_amount,
                i64::from(limit),
                i64::from(offset),
            ),
        )
        .await
        .map_err(|_| "Failed to query records".to_string())?;

    let category_name_map: HashMap<String, String> = categories
        .iter()
        .map(|category| (category.id.clone(), category.name.clone()))
        .collect();

    let mut records_output = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query records".to_string())?
    {
        let record = records::extract_record_from_row(row).map_err(|(_, message)| message)?;
        let category_name = category_name_map
            .get(&record.category_id)
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        records_output.push(json!({
            "id": record.id,
            "name": record.name,
            "amount": record.amount,
            "category_id": record.category_id,
            "category_name": category_name,
            "date": record.date,
        }));
    }

    Ok(json!({
        "ok": true,
        "total_count": total_count,
        "limit": limit,
        "offset": offset,
        "filters": {
            "start_date": start_date,
            "end_date": end_date,
            "category_id": if category_filter.is_empty() { serde_json::Value::Null } else { json!(category_filter) },
            "name_contains": if name_filter.is_empty() { serde_json::Value::Null } else { json!(name_filter) },
            "min_amount": if input.min_amount.is_some() { json!(min_amount) } else { serde_json::Value::Null },
            "max_amount": if input.max_amount.is_some() { json!(max_amount) } else { serde_json::Value::Null }
        },
        "records": records_output
    }))
}

fn resolve_category_filter_id(
    categories: &[CategoryInfo],
    category_id: Option<&str>,
    category_name: Option<&str>,
) -> Result<String, String> {
    let provided_id = category_id.unwrap_or("").trim();
    let provided_name = category_name.unwrap_or("").trim();

    if provided_id.is_empty() && provided_name.is_empty() {
        return Ok(String::new());
    }

    resolve_category_id(categories, provided_id, provided_name).ok_or_else(|| {
        format!(
            "Category not found. Available categories: {}",
            format_category_options(categories)
        )
    })
}

async fn resolve_or_create_category(
    db_pool: &DbPool,
    user_id: &str,
    categories: &[CategoryInfo],
    category_id: Option<&str>,
    category_name: Option<&str>,
    is_income: Option<bool>,
) -> Result<CategoryInfo, String> {
    let provided_id = category_id.unwrap_or("").trim();
    let provided_name = category_name.unwrap_or("").trim();

    if let Some(resolved_id) = resolve_category_id(categories, provided_id, provided_name)
        && let Some(category) = categories
            .iter()
            .find(|category| category.id == resolved_id)
    {
        return Ok(category.clone());
    }

    if !provided_name.is_empty()
        && let Some(income_flag) = is_income
    {
        return get_or_create_category(db_pool, user_id, provided_name, income_flag).await;
    }

    if categories.is_empty() {
        return Err(
            "No categories found. Please provide category_name and is_income when creating the first record."
                .to_string(),
        );
    }

    Err(format!(
        "Category not found. Available categories: {}",
        format_category_options(categories)
    ))
}

fn format_category_options(categories: &[CategoryInfo]) -> String {
    if categories.is_empty() {
        return "(none)".to_string();
    }

    categories
        .iter()
        .map(|category| format!("{} ({})", category.name, category.id))
        .collect::<Vec<_>>()
        .join(", ")
}

async fn fetch_record_by_exact_name(
    db_pool: &DbPool,
    user_id: &str,
    record_name: &str,
) -> Result<Record, String> {
    let trimmed = record_name.trim();
    if trimmed.is_empty() {
        return Err("record_name cannot be empty".to_string());
    }

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;

    let mut rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records WHERE LOWER(name) = LOWER(?) ORDER BY date DESC LIMIT 3",
            [trimmed],
        )
        .await
        .map_err(|_| "Failed to query record by name".to_string())?;

    let mut matches = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query record by name".to_string())?
    {
        let record = records::extract_record_from_row(row).map_err(|(_, message)| message)?;
        matches.push(record);
    }

    match matches.len() {
        0 => Err("Record not found. Please include record_id.".to_string()),
        1 => Ok(matches.remove(0)),
        _ => Err(format!(
            "Multiple records match that name. Please use record_id. Matching ids: {}",
            matches
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

async fn get_category_is_income(
    conn: &libsql::Connection,
    category_id: &str,
) -> Result<bool, String> {
    let mut rows = conn
        .query(
            "SELECT is_income FROM categories WHERE id = ?",
            [category_id],
        )
        .await
        .map_err(|_| "Failed to query category type".to_string())?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query category type".to_string())?
    {
        row.get(0).map_err(|_| "Invalid category data".to_string())
    } else {
        Err("Category does not exist".to_string())
    }
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
