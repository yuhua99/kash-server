use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use libsql::Row;
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::get_current_user;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{
    CreateRecordPayload, GetRecordsQuery, GetRecordsResponse, Record, UpdateRecordPayload,
};
use crate::validation::{
    validate_category_exists, validate_currency, validate_date, validate_offset,
    validate_records_limit,
};

use super::validation::{
    get_category_is_income, normalize_amount_by_category, validate_category_id,
    validate_record_amount, validate_record_name,
};

pub fn extract_record_from_row(row: Row) -> Result<Record, (StatusCode, String)> {
    let id: String = row
        .get(0)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let name: String = row
        .get(1)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let amount: f64 = row
        .get(2)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let currency: String = row
        .get(3)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let category_id: Option<String> = row
        .get(4)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let date: String = row
        .get(5)
        .map_err(|_| db_error_with_context("invalid record data"))?;

    Ok(Record {
        id,
        name,
        amount,
        currency,
        category_id,
        date,
    })
}

pub async fn create_record_for_user(
    db: &crate::Db,
    user_id: &str,
    payload: CreateRecordPayload,
) -> Result<Record, (StatusCode, String)> {
    validate_record_name(&payload.name)?;
    validate_record_amount(payload.amount)?;
    let currency = validate_currency(&payload.currency)?;
    validate_category_id(&payload.category_id)?;
    validate_date(&payload.date)?;

    let category_id = payload.category_id.trim().to_string();

    validate_category_exists(db, user_id, &category_id).await?;

    let is_income = {
        let conn = db.connect().map_err(|_| db_error())?;
        get_category_is_income(&conn, user_id, &category_id).await?
    };
    let normalized_amount = normalize_amount_by_category(payload.amount, is_income);

    let record_id = Uuid::new_v4().to_string();

    let conn = db.connect().map_err(|_| db_error())?;
    conn.execute(
        "INSERT INTO records (id, owner_user_id, name, amount, currency, category_id, date) VALUES (?, ?, ?, ?, ?, ?, ?)",
        (
            record_id.as_str(),
            user_id,
            payload.name.trim(),
            normalized_amount,
            currency.as_str(),
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
        currency,
        category_id: Some(category_id),
        date: payload.date.trim().to_string(),
    })
}

pub async fn create_record(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateRecordPayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let record = create_record_for_user(&app_state.main_db, &user.id, payload).await?;
    Ok((StatusCode::CREATED, Json(record)))
}

pub async fn get_records(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<GetRecordsQuery>,
) -> Result<(StatusCode, Json<GetRecordsResponse>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;

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
                    "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str()),
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
                    "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND pending = ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), p),
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
                    "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND settle = ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), s),
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
                .query(
                    "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND pending = ? AND settle = ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), p, s),
                )
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
                .query(
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? ORDER BY date DESC LIMIT ? OFFSET ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), limit, offset),
                )
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
        (Some(p), None) => {
            let mut rows = conn
                .query(
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND pending = ? ORDER BY date DESC LIMIT ? OFFSET ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), p, limit, offset),
                )
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
        (None, Some(s)) => {
            let mut rows = conn
                .query(
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND settle = ? ORDER BY date DESC LIMIT ? OFFSET ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), s, limit, offset),
                )
                .await
                .map_err(|_| db_error_with_context("failed to query records"))?;

            while let Some(row) = rows.next().await.map_err(|_| db_error())? {
                records.push(extract_record_from_row(row)?);
            }
        }
        (Some(p), Some(s)) => {
            let mut rows = conn
                .query(
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND pending = ? AND settle = ? ORDER BY date DESC LIMIT ? OFFSET ?",
                    (user.id.as_str(), start_date.as_str(), end_date.as_str(), p, s, limit, offset),
                )
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
    let user = get_current_user(&session).await?;

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

    let db = &app_state.main_db;

    if let Some(ref category_id) = payload.category_id {
        validate_category_exists(db, &user.id, category_id).await?;
    }

    let conn = db.connect().map_err(|_| db_error())?;

    let mut existing_rows = conn
        .query(
            "SELECT id, name, amount, currency, category_id, date FROM records WHERE id = ? AND owner_user_id = ?",
            (record_id.as_str(), user.id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query existing record"))?;

    let existing_record = if let Some(row) = existing_rows.next().await.map_err(|_| db_error())? {
        extract_record_from_row(row)?
    } else {
        return Err((StatusCode::NOT_FOUND, "Record not found".to_string()));
    };

    let updated_name = payload.name.as_deref().unwrap_or(&existing_record.name);
    let updated_category_id = payload
        .category_id
        .clone()
        .or(existing_record.category_id.clone());
    let updated_amount = if let Some(amount) = payload.amount {
        if let Some(ref category_id) = updated_category_id {
            let is_income = get_category_is_income(&conn, &user.id, category_id).await?;
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

    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ? AND owner_user_id = ?",
            (
                updated_name,
                updated_amount,
                updated_category_id.as_deref(),
                updated_date.as_str(),
                record_id.as_str(),
                user.id.as_str(),
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to update record"))?;

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
        currency: existing_record.currency,
        category_id: updated_category_id,
        date: updated_date,
    };

    Ok((StatusCode::OK, Json(updated_record)))
}

pub async fn delete_record(
    State(app_state): State<AppState>,
    session: Session,
    Path(record_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;

    let affected_rows = conn
        .execute(
            "DELETE FROM records WHERE id = ? AND owner_user_id = ?",
            (record_id.as_str(), user.id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to delete record"))?;

    if affected_rows == 0 {
        return Err((StatusCode::NOT_FOUND, "Record not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}
