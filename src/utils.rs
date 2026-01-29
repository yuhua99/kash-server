use axum::http::StatusCode;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

use crate::constants::*;
use crate::db_pool::DbPool;

static CACHED_DATABASE_PATH: OnceLock<String> = OnceLock::new();

pub fn get_database_path() -> &'static str {
    CACHED_DATABASE_PATH.get_or_init(|| {
        std::env::var("DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DATA_PATH.to_string())
    })
}

/// Get a user database connection from the pool
/// This replaces the old implementation that created a new connection each time
pub async fn get_user_database_from_pool(
    pool: &DbPool,
    user_id: &str,
) -> Result<Arc<RwLock<libsql::Connection>>, (StatusCode, String)> {
    pool.get_user_db(user_id).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            ERR_DATABASE_ACCESS.to_string(),
        )
    })
}

pub fn db_error() -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        ERR_DATABASE_OPERATION.to_string(),
    )
}

pub fn db_error_with_context(context: &str) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Database error: {}", context),
    )
}

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

pub async fn validate_category_exists(
    user_db: &Arc<RwLock<libsql::Connection>>,
    category_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = user_db.read().await;
    let mut rows = conn
        .query("SELECT id FROM categories WHERE id = ?", [category_id])
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
        None => Ok(0), // Default offset
    }
}
