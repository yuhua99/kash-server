use axum::http::StatusCode;

use crate::constants::*;

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

pub async fn validate_category_exists(
    db: &crate::Db,
    user_id: &str,
    category_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = db.read().await;
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

pub fn validate_friendship_transition(from: &str, to: &str) -> Result<(), String> {
    use crate::constants::*;
    match (from, to) {
        (FRIEND_STATUS_PENDING, FRIEND_STATUS_ACCEPTED) => Ok(()),
        (FRIEND_STATUS_PENDING, FRIEND_STATUS_BLOCKED) => Ok(()),
        (FRIEND_STATUS_ACCEPTED, FRIEND_STATUS_UNFRIENDED) => Ok(()),
        (FRIEND_STATUS_BLOCKED, FRIEND_STATUS_UNFRIENDED) => Ok(()),
        _ => Err(format!(
            "Invalid friendship transition from {} to {}",
            from, to
        )),
    }
}

/// Validates split participants for consistency and validity.
///
/// Checks:
/// - No duplicate user_ids (including initiator appearing in splits)
/// - All amounts are strictly positive (> 0.0)
/// - Amounts are finite (no NaN or infinity)
///
/// # Errors
/// Returns descriptive error messages for validation failures.
pub fn validate_split_participants(
    splits: &[crate::models::SplitParticipant],
    initiator_id: &str,
) -> Result<(), String> {
    // Check for duplicate user_ids and ensure initiator doesn't appear in splits
    let mut seen_ids = std::collections::HashSet::new();
    seen_ids.insert(initiator_id.to_string());

    for split in splits {
        if !seen_ids.insert(split.user_id.clone()) {
            return Err(format!("Duplicate participant: {}", split.user_id));
        }

        // Check amount is positive
        if split.amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }

        // Check for NaN or infinity
        if !split.amount.is_finite() {
            return Err("Amount must be a valid finite number".to_string());
        }
    }

    Ok(())
}

/// Calculates final split amounts with deterministic remainder assignment.
///
/// If the sum of split amounts is less than the total, the remainder is
/// assigned to the initiator. Returns all participants (including initiator)
/// with their final amounts, rounded to 2 decimals.
///
/// # Errors
/// Returns error if:
/// - Split sum exceeds total_amount
/// - Any amount is not finite
pub fn calculate_split_amounts(
    total: f64,
    splits: Vec<crate::models::SplitParticipant>,
    initiator_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    // Ensure total is valid
    if !total.is_finite() || total <= 0.0 {
        return Err("Total amount must be a positive finite number".to_string());
    }

    // Sum participant splits with rounding to 2 decimals
    let mut total_split = 0.0;
    for split in &splits {
        // Round each split to 2 decimals to avoid floating-point drift
        let rounded = (split.amount * 100.0).round() / 100.0;
        total_split += rounded;
    }

    // Round total_split to 2 decimals for comparison
    total_split = (total_split * 100.0).round() / 100.0;

    // Check that sum doesn't exceed total
    if total_split > total {
        return Err("Split sum exceeds total".to_string());
    }

    // Build result with all participants
    let mut result = Vec::new();

    // Add initiator's share (total minus all other splits, plus remainder)
    let initiator_amount = total - total_split;
    let initiator_amount = (initiator_amount * 100.0).round() / 100.0;
    result.push((initiator_id.to_string(), initiator_amount));

    // Add all other participants with rounded amounts
    for split in splits {
        let rounded = (split.amount * 100.0).round() / 100.0;
        result.push((split.user_id, rounded));
    }

    Ok(result)
}
