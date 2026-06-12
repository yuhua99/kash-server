use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::auth::get_current_user;
use crate::errors::db_error_with_context;
use crate::models::{Record, UpdateSettlePayload};
use crate::money::to_decimal;
use crate::{AppState, TransactionError, with_transaction};

enum UpdateSettleError {
    Transaction(TransactionError),
    NotFound,
    Db(&'static str),
}

impl From<TransactionError> for UpdateSettleError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<UpdateSettleError> for (StatusCode, String) {
    fn from(value: UpdateSettleError) -> Self {
        match value {
            UpdateSettleError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            UpdateSettleError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            UpdateSettleError::NotFound => (StatusCode::NOT_FOUND, "Record not found".to_string()),
            UpdateSettleError::Db(ctx) => db_error_with_context(ctx),
        }
    }
}

pub async fn update_settle(
    State(app_state): State<AppState>,
    session: Session,
    Path(record_id): Path<String>,
    Json(_payload): Json<UpdateSettlePayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = current_user.id.clone();
    let db = &app_state.main_db;

    let record = with_transaction(db, |conn| {
        let record_id = record_id.clone();
        let user_id = user_id.clone();
        let owner_user_id = user_id.clone();
        Box::pin(async move {
            let mut rows = conn
                .query(
                    "SELECT owner_user_id, id, name, amount, currency, category_id, date, settle FROM records WHERE id = ? AND (owner_user_id = ? OR creditor_user_id = ?)",
                    (record_id.as_str(), owner_user_id.as_str(), user_id.as_str()),
                )
                .await
                .map_err(|_| UpdateSettleError::Db("failed to query record"))?;

            let row = rows
                .next()
                .await
                .map_err(|_| UpdateSettleError::Db("failed to query record"))?
                .ok_or(UpdateSettleError::NotFound)?;

            let record_owner_user_id: String = row
                .get(0)
                .map_err(|_| UpdateSettleError::Db("invalid record data"))?;
            let amount_cents: i64 = row
                .get(3)
                .map_err(|_| UpdateSettleError::Db("invalid record data"))?;
            let settle: bool = row
                .get(7)
                .map_err(|_| UpdateSettleError::Db("invalid record data"))?;
            let category_id = if record_owner_user_id == user_id {
                row.get(5)
                    .map_err(|_| UpdateSettleError::Db("invalid record data"))?
            } else {
                None
            };
            let record = Record {
                id: row
                    .get(1)
                    .map_err(|_| UpdateSettleError::Db("invalid record data"))?,
                name: row
                    .get(2)
                    .map_err(|_| UpdateSettleError::Db("invalid record data"))?,
                amount: to_decimal(amount_cents),
                currency: row
                    .get(4)
                    .map_err(|_| UpdateSettleError::Db("invalid record data"))?,
                category_id,
                date: row
                    .get(6)
                    .map_err(|_| UpdateSettleError::Db("invalid record data"))?,
            };

            drop(rows);

            if settle {
                return Ok(record);
            }

            let affected_rows = conn
                .execute(
                    "UPDATE records SET settle = ? WHERE id = ? AND (owner_user_id = ? OR creditor_user_id = ?)",
                    (true, record_id.as_str(), owner_user_id.as_str(), user_id.as_str()),
                )
                .await
                .map_err(|_| UpdateSettleError::Db("failed to update settlement status"))?;

            if affected_rows != 1 {
                return Err(UpdateSettleError::NotFound);
            }

            Ok(record)
        })
    })
    .await
    .map_err(|e: UpdateSettleError| -> (StatusCode, String) { e.into() })?;

    Ok((StatusCode::OK, Json(record)))
}
