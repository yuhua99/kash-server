use axum::http::StatusCode;

use crate::constants::*;
use crate::errors::{db_error, db_error_with_context};

pub fn validate_string_length(
    value: &str,
    field_name: &str,
    max_length: usize,
) -> Result<(), (StatusCode, String)> {
    if value.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{} cannot be empty", field_name),
        ));
    }
    if value.len() > max_length {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{} must be less than {} characters", field_name, max_length),
        ));
    }
    Ok(())
}

pub fn validate_date(value: &str) -> Result<(), (StatusCode, String)> {
    if value.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Date cannot be empty".to_string()));
    }

    let format = time::format_description::parse("[year]-[month]-[day]")
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid date format".to_string()))?;

    time::Date::parse(value.trim(), &format)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid date format".to_string()))?;

    Ok(())
}

pub fn validate_currency(value: &str) -> Result<String, (StatusCode, String)> {
    if SUPPORTED_CURRENCIES.contains(&value) {
        return Ok(value.to_string());
    }

    Err((
        StatusCode::BAD_REQUEST,
        format!("Unsupported currency: {}", value),
    ))
}

pub fn validate_currency_list(value: &str) -> Result<Vec<String>, (StatusCode, String)> {
    let codes: Vec<&str> = value.split(',').collect();

    if codes.is_empty() || codes.iter().any(|code| code.is_empty()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Quotes must include at least one currency".to_string(),
        ));
    }

    codes
        .into_iter()
        .map(validate_currency)
        .collect::<Result<Vec<_>, _>>()
}

pub async fn validate_category_exists(
    db: &crate::Db,
    user_id: &str,
    category_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = crate::database::db_conn(db).await.map_err(|_| db_error())?;
    let mut rows = conn
        .query(
            "SELECT id FROM categories WHERE id = ? AND owner_user_id = ?",
            (category_id, user_id),
        )
        .await
        .map_err(|_| db_error_with_context("failed to check category existence"))?;

    if rows.next().await.map_err(|_| db_error())?.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Category does not exist".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_limit(limit: Option<u32>, default: u32) -> Result<u32, (StatusCode, String)> {
    match limit {
        Some(l) => {
            if l == 0 {
                Err((
                    StatusCode::BAD_REQUEST,
                    "Limit must be greater than 0".to_string(),
                ))
            } else if l > MAX_LIMIT {
                Err((
                    StatusCode::BAD_REQUEST,
                    format!("Limit cannot exceed {}", MAX_LIMIT),
                ))
            } else {
                Ok(l)
            }
        }
        None => Ok(default),
    }
}

pub fn validate_categories_limit(limit: Option<u32>) -> Result<u32, (StatusCode, String)> {
    validate_limit(limit, DEFAULT_CATEGORIES_LIMIT)
}

pub fn validate_records_limit(limit: Option<u32>) -> Result<u32, (StatusCode, String)> {
    validate_limit(limit, DEFAULT_RECORDS_LIMIT)
}

pub fn validate_offset(offset: Option<u32>) -> Result<u32, (StatusCode, String)> {
    match offset {
        Some(o) => {
            if o > MAX_OFFSET {
                Err((
                    StatusCode::BAD_REQUEST,
                    format!("Offset cannot exceed {}", MAX_OFFSET),
                ))
            } else {
                Ok(o)
            }
        }
        None => Ok(0),
    }
}
