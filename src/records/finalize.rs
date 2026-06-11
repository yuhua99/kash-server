use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::auth::get_current_user;
use crate::constants::MAX_RECORD_NAME_LENGTH;
use crate::errors::db_error_with_context;
use crate::models::{FinalizePendingPayload, Record};
use crate::validation::validate_string_length;
use crate::{AppState, TransactionError, with_transaction};

use super::validation::validate_category_id;

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

pub async fn finalize_pending_record(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<FinalizePendingPayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    validate_category_id(&payload.category_id)?;
    validate_string_length(&payload.record_id, "Record ID", MAX_RECORD_NAME_LENGTH)?;

    let db = &app_state.main_db;
    let category_id = payload.category_id.trim().to_string();
    let record_id = payload.record_id.trim().to_string();

    let record = with_transaction(db, |conn| {
        let category_id = category_id.clone();
        let record_id = record_id.clone();
        let owner_user_id = user.id.clone();
        Box::pin(async move {
            let mut category_rows = conn
                .query(
                    "SELECT id FROM categories WHERE id = ? AND owner_user_id = ?",
                    (category_id.as_str(), owner_user_id.as_str()),
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
                    "SELECT pending FROM records WHERE id = ? AND owner_user_id = ?",
                    (record_id.as_str(), owner_user_id.as_str()),
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
                    "UPDATE records SET pending = ?, category_id = ? WHERE id = ? AND owner_user_id = ? AND pending = ?",
                    (
                        false,
                        category_id.as_str(),
                        record_id.as_str(),
                        owner_user_id.as_str(),
                        true,
                    ),
                )
                .await
                .map_err(|_| FinalizePendingError::Db("failed to finalize pending record"))?;

            if affected_rows == 0 {
                return Err(FinalizePendingError::Conflict);
            }

            let mut updated_rows = conn
                .query(
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE id = ? AND owner_user_id = ?",
                    (record_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| FinalizePendingError::Db("failed to load finalized record"))?;

            let row = updated_rows
                .next()
                .await
                .map_err(|_| FinalizePendingError::Db("failed to load finalized record"))?
                .ok_or(FinalizePendingError::NotFound)?;

            let finalized_currency: String = row
                .get(3)
                .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?;
            let finalized_category_id: Option<String> = row
                .get(4)
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
                currency: finalized_currency,
                category_id: finalized_category_id,
                date: row
                    .get(5)
                    .map_err(|_| FinalizePendingError::Db("invalid finalized record data"))?,
            };

            Ok(record)
        })
    })
    .await
    .map_err(|e: FinalizePendingError| -> (StatusCode, String) { e.into() })?;

    Ok((StatusCode::OK, Json(record)))
}
