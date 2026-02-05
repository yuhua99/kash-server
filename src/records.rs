use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::constants::*;
use crate::models::{
    CreateRecordPayload, GetRecordsQuery, GetRecordsResponse, Record, UpdateRecordPayload,
};
use crate::utils::{
    db_error, db_error_with_context, get_user_database_from_pool, validate_category_exists,
    validate_date, validate_offset, validate_records_limit, validate_string_length,
};
use crate::{AppState, DbPool};

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
    let date: String = row
        .get(4)
        .map_err(|_| db_error_with_context("invalid record data"))?;

    Ok(Record {
        id,
        name,
        amount,
        category_id,
        date,
    })
}

pub async fn create_record_for_user(
    db_pool: &DbPool,
    user_id: &str,
    payload: CreateRecordPayload,
) -> Result<Record, (StatusCode, String)> {
    // Input validation
    validate_record_name(&payload.name)?;
    validate_record_amount(payload.amount)?;
    validate_category_id(&payload.category_id)?;
    validate_date(&payload.date)?;

    // Get user's database
    let user_db = get_user_database_from_pool(db_pool, user_id).await?;

    // Validate that the category exists
    validate_category_exists(&user_db, &payload.category_id).await?;

    // Create record
    let record_id = Uuid::new_v4().to_string();

    let conn = user_db.write().await;
    conn.execute(
        "INSERT INTO records (id, name, amount, category_id, date) VALUES (?, ?, ?, ?, ?)",
        (
            record_id.as_str(),
            payload.name.trim(),
            payload.amount,
            payload.category_id.trim(),
            payload.date.trim(),
        ),
    )
    .await
    .map_err(|_| db_error_with_context("record creation failed"))?;

    Ok(Record {
        id: record_id,
        name: payload.name.trim().to_string(),
        amount: payload.amount,
        category_id: payload.category_id.trim().to_string(),
        date: payload.date.trim().to_string(),
    })
}

pub async fn create_record(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateRecordPayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    // Get current user from session
    let user = get_current_user(&session).await?;

    let record = create_record_for_user(&app_state.db_pool, &user.id, payload).await?;

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

    if let Some(ref start_date) = query.start_date {
        validate_date(start_date)?;
    }

    if let Some(ref end_date) = query.end_date {
        validate_date(end_date)?;
    }

    let start_date = query.start_date.unwrap_or_else(|| "0000-01-01".to_string());
    let end_date = query.end_date.unwrap_or_else(|| "9999-12-31".to_string());

    // Get total count
    let count_query = "SELECT COUNT(*) FROM records WHERE date BETWEEN ? AND ?";
    let mut count_rows = conn
        .query(count_query, (start_date.as_str(), end_date.as_str()))
        .await
        .map_err(|_| db_error_with_context("failed to count records"))?;

    let total_count: u32 = if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
        row.get(0).map_err(|_| db_error())?
    } else {
        0
    };

    // Get records
    let records_query = "SELECT id, name, amount, category_id, date FROM records WHERE date BETWEEN ? AND ? ORDER BY date DESC LIMIT ? OFFSET ?";
    let mut rows = conn
        .query(
            records_query,
            (start_date.as_str(), end_date.as_str(), limit, offset),
        )
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
        && payload.date.is_none()
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

    if let Some(ref date) = payload.date {
        validate_date(date)?;
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
            "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
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
    let updated_date = payload.date.unwrap_or(existing_record.date);

    // Update the record and verify it was actually modified
    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ?",
            (
                updated_name,
                updated_amount,
                updated_category_id,
                updated_date.as_str(),
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
        date: updated_date,
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
