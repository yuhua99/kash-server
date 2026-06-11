use axum::http::StatusCode;
use libsql::Connection;

use crate::constants::{MAX_CATEGORY_NAME_LENGTH, MAX_RECORD_NAME_LENGTH};
use crate::errors::{db_error, db_error_with_context};
use crate::validation::validate_string_length;

pub fn validate_record_name(name: &str) -> Result<(), (StatusCode, String)> {
    validate_string_length(name, "Record name", MAX_RECORD_NAME_LENGTH)
}

pub fn validate_record_amount(amount: f64) -> Result<(), (StatusCode, String)> {
    if amount == 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Record amount cannot be zero".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_category_id(category_id: &str) -> Result<(), (StatusCode, String)> {
    validate_string_length(category_id, "Category ID", MAX_CATEGORY_NAME_LENGTH)
}

pub(crate) fn normalize_amount_by_category(amount: f64, is_income: bool) -> f64 {
    if is_income {
        amount.abs()
    } else {
        -amount.abs()
    }
}

pub(crate) async fn get_category_is_income(
    conn: &Connection,
    user_id: &str,
    category_id: &str,
) -> Result<bool, (StatusCode, String)> {
    let mut rows = conn
        .query(
            "SELECT is_income FROM categories WHERE id = ? AND owner_user_id = ?",
            (category_id, user_id),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query category type"))?;

    if let Some(row) = rows.next().await.map_err(|_| db_error())? {
        let is_income: bool = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid category data"))?;
        Ok(is_income)
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Category does not exist".to_string(),
        ))
    }
}
