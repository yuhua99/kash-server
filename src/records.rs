use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::get_current_user;
use crate::constants::*;
use crate::models::{
    CreateRecordPayload, GetRecordsQuery, GetRecordsResponse, Record, UpdateRecordPayload,
};
use crate::utils::{
    db_error, db_error_with_context, get_user_database_from_pool, validate_category_exists,
    validate_offset, validate_records_limit, validate_string_length,
};

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

pub fn validate_timestamp(timestamp: i64) -> Result<(), (StatusCode, String)> {
    let current_time = time::OffsetDateTime::now_utc().unix_timestamp();

    // Reject timestamps from more than 10 years in the past
    let ten_years_ago = current_time - (10 * 365 * 24 * 60 * 60);

    // Reject timestamps from more than 1 hour in the future (to allow for clock skew)
    let one_hour_future = current_time + (60 * 60);

    if timestamp < ten_years_ago {
        return Err((
            StatusCode::BAD_REQUEST,
            "Timestamp cannot be more than 10 years in the past".to_string(),
        ));
    }

    if timestamp > one_hour_future {
        return Err((
            StatusCode::BAD_REQUEST,
            "Timestamp cannot be more than 1 hour in the future".to_string(),
        ));
    }

    Ok(())
}

pub fn extract_record_from_row(row: libsql::Row) -> Result<Record, (StatusCode, String)> {
    let id: String = row
        .get(0)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let name: String = row
        .get(1)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let amount: f64 = row
        .get(2)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let category_id: String = row
        .get(3)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let timestamp: i64 = row
        .get(4)
        .map_err(|_| db_error_with_context("invalid record data"))?;

    Ok(Record {
        id,
        name,
        amount,
        category_id,
        timestamp,
    })
}

pub async fn create_record(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateRecordPayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    // Get current user from session
    let user = get_current_user(&session).await?;

    // Input validation
    validate_record_name(&payload.name)?;
    validate_record_amount(payload.amount)?;
    validate_category_id(&payload.category_id)?;
    validate_timestamp(payload.timestamp)?;

    // Get user's database from pool
    let user_db = get_user_database_from_pool(&app_state.db_pool, &user.id).await?;

    // Validate that the category exists
    validate_category_exists(&user_db, &payload.category_id).await?;

    // Create record
    let record_id = Uuid::new_v4().to_string();

    let conn = user_db.write().await;
    conn.execute(
        "INSERT INTO records (id, name, amount, category_id, timestamp) VALUES (?, ?, ?, ?, ?)",
        (
            record_id.as_str(),
            payload.name.trim(),
            payload.amount,
            payload.category_id.trim(),
            payload.timestamp,
        ),
    )
    .await
    .map_err(|_| db_error_with_context("record creation failed"))?;

    let record = Record {
        id: record_id,
        name: payload.name.trim().to_string(),
        amount: payload.amount,
        category_id: payload.category_id.trim().to_string(),
        timestamp: payload.timestamp,
    };

    Ok((StatusCode::CREATED, Json(record)))
}

pub async fn get_records(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<GetRecordsQuery>,
) -> Result<(StatusCode, Json<GetRecordsResponse>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;

    let user_db = get_user_database_from_pool(&app_state.db_pool, &user.id).await?;

    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;

    let conn = user_db.read().await;

    // Use default values: start_time defaults to 0, end_time defaults to current timestamp
    let start_time = query.start_time.unwrap_or(0);
    let end_time = query
        .end_time
        .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());

    // Get total count
    let count_query = "SELECT COUNT(*) FROM records WHERE timestamp BETWEEN ? AND ?";
    let mut count_rows = conn
        .query(count_query, (start_time, end_time))
        .await
        .map_err(|_| db_error_with_context("failed to count records"))?;

    let total_count: u32 = if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
        row.get(0).map_err(|_| db_error())?
    } else {
        0
    };

    // Get records
    let records_query = "SELECT id, name, amount, category_id, timestamp FROM records WHERE timestamp BETWEEN ? AND ? ORDER BY timestamp DESC LIMIT ? OFFSET ?";
    let mut rows = conn
        .query(records_query, (start_time, end_time, limit, offset))
        .await
        .map_err(|_| db_error_with_context("failed to query records"))?;

    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| db_error())? {
        records.push(extract_record_from_row(row)?);
    }

    Ok((
        StatusCode::OK,
        Json(GetRecordsResponse {
            records,
            total_count,
        }),
    ))
}

pub async fn update_record(
    State(app_state): State<AppState>,
    session: Session,
    Path(record_id): Path<String>,
    Json(payload): Json<UpdateRecordPayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    // Get current user from session
    let user = get_current_user(&session).await?;

    // Validate that at least one field is being updated
    if payload.name.is_none()
        && payload.amount.is_none()
        && payload.category_id.is_none()
        && payload.timestamp.is_none()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one field must be provided for update".to_string(),
        ));
    }

    // Input validation for provided fields
    if let Some(ref name) = payload.name {
        validate_record_name(name)?;
    }

    if let Some(amount) = payload.amount {
        validate_record_amount(amount)?;
    }

    if let Some(ref category_id) = payload.category_id {
        validate_category_id(category_id)?;
    }

    // Get user's database
    let user_db = get_user_database_from_pool(&app_state.db_pool, &user.id).await?;

    // Validate that the category exists if being updated
    if let Some(ref category_id) = payload.category_id {
        validate_category_exists(&user_db, category_id).await?;
    }

    let conn = user_db.write().await;

    // First, check if the record exists and belongs to the user
    let mut existing_rows = conn
        .query(
            "SELECT id, name, amount, category_id, timestamp FROM records WHERE id = ?",
            [record_id.as_str()],
        )
        .await
        .map_err(|_| db_error_with_context("failed to query existing record"))?;

    let existing_record = if let Some(row) = existing_rows.next().await.map_err(|_| db_error())? {
        extract_record_from_row(row)?
    } else {
        return Err((StatusCode::NOT_FOUND, "Record not found".to_string()));
    };

    // Build the updated record with new values or keep existing ones
    let updated_name = payload.name.as_deref().unwrap_or(&existing_record.name);
    let updated_amount = payload.amount.unwrap_or(existing_record.amount);
    let updated_category_id = payload
        .category_id
        .as_deref()
        .unwrap_or(&existing_record.category_id);
    let updated_timestamp = payload.timestamp.unwrap_or(existing_record.timestamp);

    // Update the record and verify it was actually modified
    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, timestamp = ? WHERE id = ?",
            (
                updated_name,
                updated_amount,
                updated_category_id,
                updated_timestamp,
                record_id.as_str(),
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to update record"))?;

    // Verify the update actually modified a record
    if affected_rows == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Record not found or no changes made".to_string(),
        ));
    }

    let updated_record = Record {
        id: record_id,
        name: updated_name.to_string(),
        amount: updated_amount,
        category_id: updated_category_id.to_string(),
        timestamp: updated_timestamp,
    };

    Ok((StatusCode::OK, Json(updated_record)))
}

pub async fn delete_record(
    State(app_state): State<AppState>,
    session: Session,
    Path(record_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Get current user from session
    let user = get_current_user(&session).await?;

    // Get user's database
    let user_db = get_user_database_from_pool(&app_state.db_pool, &user.id).await?;

    let conn = user_db.write().await;

    // Delete the record and verify it was actually deleted
    let affected_rows = conn
        .execute("DELETE FROM records WHERE id = ?", [record_id.as_str()])
        .await
        .map_err(|_| db_error_with_context("failed to delete record"))?;

    // Verify the delete actually removed a record
    if affected_rows == 0 {
        return Err((StatusCode::NOT_FOUND, "Record not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}
