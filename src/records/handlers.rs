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
use crate::money::{to_cents, to_decimal};
use crate::validation::{
    validate_currency, validate_date, validate_offset, validate_records_limit,
};
use crate::{TransactionError, with_transaction};

use super::validation::{
    get_category_is_income, normalize_amount_by_category, validate_category_id,
    validate_record_amount, validate_record_name,
};

enum CreateRecordError {
    Transaction(TransactionError),
    CategoryMissing,
    Db(&'static str),
}

impl From<TransactionError> for CreateRecordError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<CreateRecordError> for (StatusCode, String) {
    fn from(value: CreateRecordError) -> Self {
        match value {
            CreateRecordError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            CreateRecordError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            CreateRecordError::CategoryMissing => (
                StatusCode::BAD_REQUEST,
                "Category does not exist".to_string(),
            ),
            CreateRecordError::Db(ctx) => db_error_with_context(ctx),
        }
    }
}

enum UpdateRecordError {
    Transaction(TransactionError),
    Db(&'static str),
    RecordNotFound,
    SplitRecord,
    CategoryMissing,
    AmountWithoutCategory,
    NoChanges,
}

impl From<TransactionError> for UpdateRecordError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<UpdateRecordError> for (StatusCode, String) {
    fn from(value: UpdateRecordError) -> Self {
        match value {
            UpdateRecordError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            UpdateRecordError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            UpdateRecordError::Db(ctx) => db_error_with_context(ctx),
            UpdateRecordError::RecordNotFound => {
                (StatusCode::NOT_FOUND, "Record not found".to_string())
            }
            UpdateRecordError::SplitRecord => (
                StatusCode::CONFLICT,
                "Split records cannot be modified directly".to_string(),
            ),
            UpdateRecordError::CategoryMissing => (
                StatusCode::BAD_REQUEST,
                "Category does not exist".to_string(),
            ),
            UpdateRecordError::AmountWithoutCategory => (
                StatusCode::BAD_REQUEST,
                "Cannot update amount without a category".to_string(),
            ),
            UpdateRecordError::NoChanges => (
                StatusCode::NOT_FOUND,
                "Record not found or no changes made".to_string(),
            ),
        }
    }
}

fn extract_record_from_row(row: Row) -> Result<Record, (StatusCode, String)> {
    let id: String = row
        .get(0)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let name: String = row
        .get(1)
        .map_err(|_| db_error_with_context("invalid record data"))?;
    let amount_cents: i64 = row
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
        amount: to_decimal(amount_cents),
        currency,
        category_id,
        date,
    })
}

async fn create_record_for_user(
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

    let record_id = Uuid::new_v4().to_string();
    let name = payload.name.trim().to_string();
    let date = payload.date.trim().to_string();
    let amount = payload.amount;
    let owner_user_id = user_id.to_string();

    with_transaction(db, |conn| {
        let record_id = record_id.clone();
        let owner_user_id = owner_user_id.clone();
        let category_id = category_id.clone();
        let name = name.clone();
        let currency = currency.clone();
        let date = date.clone();
        Box::pin(async move {
            let is_income = get_category_is_income(conn, &owner_user_id, &category_id)
                .await
                .map_err(|e| {
                    if e.0 == StatusCode::BAD_REQUEST {
                        CreateRecordError::CategoryMissing
                    } else {
                        CreateRecordError::Db("failed to query category type")
                    }
                })?;
            let normalized_amount = normalize_amount_by_category(to_cents(amount), is_income);

            conn.execute(
                "INSERT INTO records (id, owner_user_id, name, amount, currency, category_id, date) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    record_id.as_str(),
                    owner_user_id.as_str(),
                    name.as_str(),
                    normalized_amount,
                    currency.as_str(),
                    category_id.as_str(),
                    date.as_str(),
                ),
            )
            .await
            .map_err(|_| CreateRecordError::Db("record creation failed"))?;

            Ok::<Record, CreateRecordError>(Record {
                id: record_id,
                name,
                amount: to_decimal(normalized_amount),
                currency,
                category_id: Some(category_id),
                date,
            })
        })
    })
    .await
    .map_err(<(StatusCode, String)>::from)
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
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    if let Some(ref start_date) = query.start_date {
        validate_date(start_date)?;
    }

    if let Some(ref end_date) = query.end_date {
        validate_date(end_date)?;
    }

    let start_date = query.start_date.unwrap_or_else(|| "0000-01-01".to_string());
    let end_date = query.end_date.unwrap_or_else(|| "9999-12-31".to_string());

    let pending = query.pending;
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
                    "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND (split_id IS NOT NULL AND category_id IS NULL) = ?",
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
                    "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND (split_id IS NOT NULL AND category_id IS NULL) = ? AND settle = ?",
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
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND (split_id IS NOT NULL AND category_id IS NULL) = ? ORDER BY date DESC LIMIT ? OFFSET ?",
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
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE owner_user_id = ? AND date BETWEEN ? AND ? AND (split_id IS NOT NULL AND category_id IS NULL) = ? AND settle = ? ORDER BY date DESC LIMIT ? OFFSET ?",
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
    let owner_user_id = user.id.clone();
    let name = payload.name.clone();
    let amount = payload.amount;
    let category_id = payload.category_id.clone();
    let date = payload.date.clone();

    let updated_record = with_transaction(db, |conn| {
        let record_id = record_id.clone();
        let owner_user_id = owner_user_id.clone();
        let name = name.clone();
        let category_id = category_id.clone();
        let date = date.clone();
        Box::pin(async move {
            let mut existing_rows = conn
                .query(
                    "SELECT id, name, amount, currency, category_id, date, split_id FROM records WHERE id = ? AND owner_user_id = ?",
                    (record_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| UpdateRecordError::Db("failed to query existing record"))?;

            let existing_record = if let Some(row) = existing_rows
                .next()
                .await
                .map_err(|_| UpdateRecordError::Db("failed to query existing record"))?
            {
                let split_id: Option<String> = row
                    .get(6)
                    .map_err(|_| UpdateRecordError::Db("invalid record data"))?;
                if split_id.is_some() {
                    return Err(UpdateRecordError::SplitRecord);
                }
                let amount_cents: i64 = row
                    .get(2)
                    .map_err(|_| UpdateRecordError::Db("invalid record data"))?;
                Record {
                    id: row
                        .get(0)
                        .map_err(|_| UpdateRecordError::Db("invalid record data"))?,
                    name: row
                        .get(1)
                        .map_err(|_| UpdateRecordError::Db("invalid record data"))?,
                    amount: to_decimal(amount_cents),
                    currency: row
                        .get(3)
                        .map_err(|_| UpdateRecordError::Db("invalid record data"))?,
                    category_id: row
                        .get(4)
                        .map_err(|_| UpdateRecordError::Db("invalid record data"))?,
                    date: row
                        .get(5)
                        .map_err(|_| UpdateRecordError::Db("invalid record data"))?,
                }
            } else {
                return Err(UpdateRecordError::RecordNotFound);
            };
            drop(existing_rows);

            let category_changed = category_id.is_some();
            let updated_name = name.unwrap_or(existing_record.name.clone());
            let updated_category_id = category_id.or(existing_record.category_id.clone());
            let updated_amount = if amount.is_some() || category_changed {
                if let Some(ref category_id) = updated_category_id {
                    let amount = amount.unwrap_or(existing_record.amount);
                    let is_income = get_category_is_income(conn, &owner_user_id, category_id)
                        .await
                        .map_err(|e| {
                            if e.0 == StatusCode::BAD_REQUEST {
                                UpdateRecordError::CategoryMissing
                            } else {
                                UpdateRecordError::Db("failed to query category type")
                            }
                        })?;
                    normalize_amount_by_category(to_cents(amount), is_income)
                } else {
                    return Err(UpdateRecordError::AmountWithoutCategory);
                }
            } else {
                to_cents(existing_record.amount)
            };
            let updated_date = date.unwrap_or(existing_record.date);

            let affected_rows = conn
                .execute(
                    "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ? AND owner_user_id = ? AND split_id IS NULL",
                    (
                        updated_name.as_str(),
                        updated_amount,
                        updated_category_id.as_deref(),
                        updated_date.as_str(),
                        record_id.as_str(),
                        owner_user_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| UpdateRecordError::Db("failed to update record"))?;

            if affected_rows == 0 {
                return Err(UpdateRecordError::NoChanges);
            }

            Ok::<Record, UpdateRecordError>(Record {
                id: record_id,
                name: updated_name,
                amount: to_decimal(updated_amount),
                currency: existing_record.currency,
                category_id: updated_category_id,
                date: updated_date,
            })
        })
    })
    .await
    .map_err(<(StatusCode, String)>::from)?;

    Ok((StatusCode::OK, Json(updated_record)))
}

pub async fn delete_record(
    State(app_state): State<AppState>,
    session: Session,
    Path(record_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    let mut rows = conn
        .query(
            "SELECT split_id FROM records WHERE id = ? AND owner_user_id = ?",
            (record_id.as_str(), user.id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query record"))?;

    let split_id: Option<String> = if let Some(row) = rows.next().await.map_err(|_| db_error())? {
        row.get(0)
            .map_err(|_| db_error_with_context("invalid record data"))?
    } else {
        return Err((StatusCode::NOT_FOUND, "Record not found".to_string()));
    };

    if split_id.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Split records cannot be deleted directly".to_string(),
        ));
    }

    drop(rows);

    let affected_rows = conn
        .execute(
            "DELETE FROM records WHERE id = ? AND owner_user_id = ? AND split_id IS NULL",
            (record_id.as_str(), user.id.as_str()),
        )
        .await
        .map_err(|_| db_error_with_context("failed to delete record"))?;

    if affected_rows == 0 {
        return Err((StatusCode::NOT_FOUND, "Record not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}
