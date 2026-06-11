use axum::http::StatusCode;

use crate::constants::ERR_DATABASE_OPERATION;

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
