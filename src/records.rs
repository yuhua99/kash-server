use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::constants::*;
use crate::db_pool::{TransactionError, with_transaction};
use crate::models::{
    CreateRecordPayload, FinalizePendingPayload, GetRecordsQuery, GetRecordsResponse, Record,
    UpdateRecordPayload, UpdateSettlePayload,
};
use crate::utils::{
    db_error, db_error_with_context, get_user_database_from_pool, validate_category_exists,
    validate_date, validate_offset, validate_records_limit, validate_string_length,
};
use crate::{AppState, DbPool};

enum FinalizePendingError {
    Transaction(TransactionError),
    Db(&'static str),
    NotFound,
    CategoryNotFound,
    Conflict,
}

impl From<TransactionError> for FinalizePendingError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<FinalizePendingError> for (StatusCode, String) {
    fn from(value: FinalizePendingError) -> Self {
        match value {
            FinalizePendingError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            FinalizePendingError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            FinalizePendingError::Db(ctx) => db_error_with_context(ctx),
            FinalizePendingError::NotFound => {
                (StatusCode::NOT_FOUND, "Record not found".to_string())
            }
            FinalizePendingError::CategoryNotFound => (
                StatusCode::BAD_REQUEST,
                "Category does not exist".to_string(),
            ),
            FinalizePendingError::Conflict => (
                StatusCode::CONFLICT,
                "Record already finalized or being finalized".to_string(),
            ),
        }
    }
}

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

fn normalize_amount_by_category(amount: f64, is_income: bool) -> f64 {
    if is_income {
        amount.abs()
    } else {
        -amount.abs()
    }
}

async fn get_category_is_income(
    conn: &libsql::Connection,
    category_id: &str,
) -> Result<bool, (StatusCode, String)> {
    let mut rows = conn
        .query(
            "SELECT is_income FROM categories WHERE id = ?",
            [category_id],
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
    let category_id: Option<String> = row
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

    let category_id = payload.category_id.trim().to_string();

    // Validate that the category exists
    validate_category_exists(&user_db, &category_id).await?;

    let is_income = {
        let conn = user_db.read().await;
        get_category_is_income(&conn, &category_id).await?
    };
    let normalized_amount = normalize_amount_by_category(payload.amount, is_income);

    // Create record
    let record_id = Uuid::new_v4().to_string();

    let conn = user_db.write().await;
    conn.execute(
        "INSERT INTO records (id, name, amount, category_id, date) VALUES (?, ?, ?, ?, ?)",
        (
            record_id.as_str(),
            payload.name.trim(),
            normalized_amount,
            category_id.as_str(),
            payload.date.trim(),
        ),
    )
    .await
    .map_err(|_| db_error_with_context("record creation failed"))?;

    Ok(Record {
        id: record_id,
        name: payload.name.trim().to_string(),
        amount: normalized_amount,
        category_id: Some(category_id),
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

    let pending = query.pending.map(|p| if p { 1 } else { 0 });
    let settle = query.settle.map(|s| if s { 1 } else { 0 });

    let total_count: u32 = match (pending, settle) {
        (None, None) => {
            let mut count_rows = conn
                .query(
                    "SELECT COUNT(*) FROM records WHERE date BETWEEN ? AND ?",
                    (start_date.as_str(), end_date.as_str()),
                )
                .await
                .map_err(|_| db_error_with_context("failed to count records"))?;

            if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
                row.get(0).map_err(|_| db_error())?
            } else {
                0
            }
        }
        (Some(p), None) => {
            let mut count_rows = conn
                .query(
                    "SELECT COUNT(*) FROM records WHERE date BETWEEN ? AND ? AND pending = ?",
                    (start_date.as_str(), end_date.as_str(), p),
                )
                .await
                .map_err(|_| db_error_with_context("failed to count records"))?;

            if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
                row.get(0).map_err(|_| db_error())?
            } else {
                0
            }
        }
        (None, Some(s)) => {
            let mut count_rows = conn
                .query(
                    "SELECT COUNT(*) FROM records WHERE date BETWEEN ? AND ? AND settle = ?",
                    (start_date.as_str(), end_date.as_str(), s),
                )
                .await
                .map_err(|_| db_error_with_context("failed to count records"))?;

            if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
                row.get(0).map_err(|_| db_error())?
            } else {
                0
            }
        }
        (Some(p), Some(s)) => {
            let mut count_rows = conn
                .query("SELECT COUNT(*) FROM records WHERE date BETWEEN ? AND ? AND pending = ? AND settle = ?", (start_date.as_str(), end_date.as_str(), p, s))
                .await
                .map_err(|_| db_error_with_context("failed to count records"))?;

            if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
                row.get(0).map_err(|_| db_error())?
            } else {
                0
            }
        }
    };

    let mut records = Vec::new();
    match (pending, settle) {
        (None, None) => {
            let mut rows = conn
                .query("SELECT id, name, amount, category_id, date FROM records WHERE date BETWEEN ? AND ? ORDER BY date DESC LIMIT ? OFFSET ?", (start_date.as_str(), end_date.as_str(), limit, offset))
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
        (Some(p), None) => {
            let mut rows = conn
                .query("SELECT id, name, amount, category_id, date FROM records WHERE date BETWEEN ? AND ? AND pending = ? ORDER BY date DESC LIMIT ? OFFSET ?", (start_date.as_str(), end_date.as_str(), p, limit, offset))
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
        (None, Some(s)) => {
            let mut rows = conn
                .query("SELECT id, name, amount, category_id, date FROM records WHERE date BETWEEN ? AND ? AND settle = ? ORDER BY date DESC LIMIT ? OFFSET ?", (start_date.as_str(), end_date.as_str(), s, limit, offset))
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
        (Some(p), Some(s)) => {
            let mut rows = conn
                .query("SELECT id, name, amount, category_id, date FROM records WHERE date BETWEEN ? AND ? AND pending = ? AND settle = ? ORDER BY date DESC LIMIT ? OFFSET ?", (start_date.as_str(), end_date.as_str(), p, s, limit, offset))
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
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
    let updated_category_id = payload
        .category_id
        .clone()
        .or(existing_record.category_id.clone());
    let updated_amount = if let Some(amount) = payload.amount {
        if let Some(ref category_id) = updated_category_id {
            let is_income = get_category_is_income(&conn, category_id).await?;
            normalize_amount_by_category(amount, is_income)
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                "Cannot update amount without a category".to_string(),
            ));
        }
    } else {
        existing_record.amount
    };
    let updated_date = payload.date.unwrap_or(existing_record.date);

    // Update the record and verify it was actually modified
    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ?",
            (
                updated_name,
                updated_amount,
                updated_category_id.as_deref(),
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
        category_id: updated_category_id,
        date: updated_date,
    };

    Ok((StatusCode::OK, Json(updated_record)))
}

pub async fn finalize_pending_record(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<FinalizePendingPayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    validate_category_id(&payload.category_id)?;
    validate_string_length(&payload.record_id, "Record ID", MAX_RECORD_NAME_LENGTH)?;

    let user_db = get_user_database_from_pool(&app_state.db_pool, &user.id).await?;
    let category_id = payload.category_id.trim().to_string();
    let record_id = payload.record_id.trim().to_string();

    let record = with_transaction(&user_db, |conn| {
        let category_id = category_id.clone();
        let record_id = record_id.clone();

        Box::pin(async move {
            let mut category_rows = conn
                .query(
                    "SELECT id FROM categories WHERE id = ?",
                    [category_id.as_str()],
                )
                .await
                .map_err(|_| FinalizePendingError::Db("failed to validate category"))?;

            if category_rows
                .next()
                .await
                .map_err(|_| FinalizePendingError::Db("failed to validate category"))?
                .is_none()
            {
                return Err(FinalizePendingError::CategoryNotFound);
            }

            let mut existing_rows = conn
                .query(
                    "SELECT pending FROM records WHERE id = ?",
                    [record_id.as_str()],
                )
                .await
                .map_err(|_| FinalizePendingError::Db("failed to query pending record"))?;

            let pending: bool = if let Some(row) = existing_rows
                .next()
                .await
                .map_err(|_| FinalizePendingError::Db("failed to query pending record"))?
            {
                row.get(0)
                    .map_err(|_| FinalizePendingError::Db("invalid pending record data"))?
            } else {
                return Err(FinalizePendingError::NotFound);
            };

            if !pending {
                return Err(FinalizePendingError::Conflict);
            }

            let affected_rows = conn
                .execute(
                    "UPDATE records SET pending = ?, category_id = ? WHERE id = ? AND pending = ?",
                    (false, category_id.as_str(), record_id.as_str(), true),
                )
                .await
                .map_err(|_| FinalizePendingError::Db("failed to finalize pending record"))?;

            if affected_rows == 0 {
                return Err(FinalizePendingError::Conflict);
            }

            let mut updated_rows = conn
                .query(
                    "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
                    [record_id.as_str()],
                )
                .await
                .map_err(|_| FinalizePendingError::Db("failed to load finalized record"))?;

            let row = updated_rows
                .next()
                .await
                .map_err(|_| FinalizePendingError::Db("failed to load finalized record"))?
                .ok_or(FinalizePendingError::NotFound)?;

            let finalized_category_id: Option<String> = row
                .get(3)
                .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?;

            let record = Record {
                id: row
                    .get(0)
                    .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?,
                name: row
                    .get(1)
                    .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?,
                amount: row
                    .get(2)
                    .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?,
                category_id: finalized_category_id,
                date: row
                    .get(4)
                    .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?,
            };

            Ok(record)
        })
    })
    .await
    .map_err(|e: FinalizePendingError| -> (StatusCode, String) { e.into() })?;

    Ok((StatusCode::OK, Json(record)))
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

pub async fn update_settle(
    State(app_state): State<AppState>,
    session: Session,
    Path(record_id): Path<String>,
    Json(_payload): Json<UpdateSettlePayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = current_user.id.clone();

    let user_db = get_user_database_from_pool(&app_state.db_pool, &user_id).await?;

    let record = with_transaction(&user_db, |conn| {
        let record_id = record_id.clone();
        let user_id = user_id.clone();
        Box::pin(async move {
            let mut rows = conn
                .query(
                    "SELECT id, name, amount, category_id, date, settle, debtor_user_id, creditor_user_id FROM records WHERE id = ?",
                    [record_id.as_str()],
                )
                .await
                .map_err(|_| TransactionError::Begin)?;

            let row = rows
                .next()
                .await
                .map_err(|_| TransactionError::Begin)?
                .ok_or(TransactionError::Begin)?;

            let settle: bool = row.get(5).map_err(|_| TransactionError::Begin)?;
            let debtor_user_id: Option<String> = row.get(6).map_err(|_| TransactionError::Begin)?;
            let creditor_user_id: Option<String> = row.get(7).map_err(|_| TransactionError::Begin)?;

            drop(rows);

            let is_owner = true;
            let is_debtor = debtor_user_id.as_ref() == Some(&user_id);
            let is_creditor = creditor_user_id.as_ref() == Some(&user_id);

            if !is_owner && !is_debtor && !is_creditor {
                return Err(TransactionError::Begin);
            }

            if settle {
                let record = Record {
                    id: row.get(0).map_err(|_| TransactionError::Begin)?,
                    name: row.get(1).map_err(|_| TransactionError::Begin)?,
                    amount: row.get(2).map_err(|_| TransactionError::Begin)?,
                    category_id: row.get(3).map_err(|_| TransactionError::Begin)?,
                    date: row.get(4).map_err(|_| TransactionError::Begin)?,
                };
                return Ok(record);
            }

            conn.execute(
                "UPDATE records SET settle = ? WHERE id = ?",
                (true, record_id.as_str()),
            )
            .await
            .map_err(|_| TransactionError::Commit)?;

            let mut updated_rows = conn
                .query(
                    "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
                    [record_id.as_str()],
                )
                .await
                .map_err(|_| TransactionError::Commit)?;

            let updated_row = updated_rows
                .next()
                .await
                .map_err(|_| TransactionError::Commit)?
                .ok_or(TransactionError::Commit)?;

            let record = Record {
                id: updated_row.get(0).map_err(|_| TransactionError::Commit)?,
                name: updated_row.get(1).map_err(|_| TransactionError::Commit)?,
                amount: updated_row.get(2).map_err(|_| TransactionError::Commit)?,
                category_id: updated_row.get(3).map_err(|_| TransactionError::Commit)?,
                date: updated_row.get(4).map_err(|_| TransactionError::Commit)?,
            };

            Ok(record)
        })
    })
    .await
    .map_err(|e| match e {
        TransactionError::Begin => (StatusCode::NOT_FOUND, "Record not found".to_string()),
        TransactionError::Commit => db_error_with_context("failed to update settlement status"),
    })?;

    Ok((StatusCode::OK, Json(record)))
}
