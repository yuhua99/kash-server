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
                    "SELECT id, name, amount, currency, category_id, date, settle FROM records WHERE id = ? AND (owner_user_id = ? OR creditor_user_id = ?)",
                    (record_id.as_str(), owner_user_id.as_str(), user_id.as_str()),
                )
                .await
                .map_err(|_| TransactionError::Begin)?;

            let row = rows
                .next()
                .await
                .map_err(|_| TransactionError::Begin)?
                .ok_or(TransactionError::Begin)?;

            let settle: bool = row.get(6).map_err(|_| TransactionError::Begin)?;

            drop(rows);

            if settle {
                let amount_cents: i64 = row.get(2).map_err(|_| TransactionError::Begin)?;
                let record = Record {
                    id: row.get(0).map_err(|_| TransactionError::Begin)?,
                    name: row.get(1).map_err(|_| TransactionError::Begin)?,
                    amount: to_decimal(amount_cents),
                    currency: row.get(3).map_err(|_| TransactionError::Begin)?,
                    category_id: row.get(4).map_err(|_| TransactionError::Begin)?,
                    date: row.get(5).map_err(|_| TransactionError::Begin)?,
                };
                return Ok(record);
            }

            conn.execute(
                "UPDATE records SET settle = ? WHERE id = ? AND (owner_user_id = ? OR creditor_user_id = ?)",
                (true, record_id.as_str(), owner_user_id.as_str(), user_id.as_str()),
            )
            .await
            .map_err(|_| TransactionError::Commit)?;

            let mut updated_rows = conn
                .query(
                    "SELECT id, name, amount, currency, category_id, date FROM records WHERE id = ? AND (owner_user_id = ? OR creditor_user_id = ?)",
                    (record_id.as_str(), owner_user_id.as_str(), user_id.as_str()),
                )
                .await
                .map_err(|_| TransactionError::Commit)?;

            let updated_row = updated_rows
                .next()
                .await
                .map_err(|_| TransactionError::Commit)?
                .ok_or(TransactionError::Commit)?;

            let amount_cents: i64 = updated_row.get(2).map_err(|_| TransactionError::Commit)?;
            let record = Record {
                id: updated_row.get(0).map_err(|_| TransactionError::Commit)?,
                name: updated_row.get(1).map_err(|_| TransactionError::Commit)?,
                amount: to_decimal(amount_cents),
                currency: updated_row.get(3).map_err(|_| TransactionError::Commit)?,
                category_id: updated_row.get(4).map_err(|_| TransactionError::Commit)?,
                date: updated_row.get(5).map_err(|_| TransactionError::Commit)?,
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
