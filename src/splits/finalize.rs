use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::errors::db_error_with_context;
use crate::models::{FinalizeSharePayload, Record};
use crate::money::to_decimal;
use crate::validation::validate_string_length;
use crate::{AppState, TransactionError, with_transaction};

struct ShareToFinalize {
    debtor_user_id: String,
    amount: i64,
    finalized_record_id: Option<String>,
    description: String,
    currency: String,
    date: String,
}

enum FinalizeShareError {
    Transaction(TransactionError),
    Db(&'static str),
    NotFound,
    CategoryNotFound,
    Conflict,
}

impl From<TransactionError> for FinalizeShareError {
    fn from(e: TransactionError) -> Self {
        Self::Transaction(e)
    }
}

impl From<FinalizeShareError> for (StatusCode, String) {
    fn from(e: FinalizeShareError) -> Self {
        match e {
            FinalizeShareError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            FinalizeShareError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            FinalizeShareError::Db(context) => db_error_with_context(context),
            FinalizeShareError::NotFound => (StatusCode::NOT_FOUND, "Share not found".to_string()),
            FinalizeShareError::CategoryNotFound => (
                StatusCode::BAD_REQUEST,
                "Category does not exist".to_string(),
            ),
            FinalizeShareError::Conflict => {
                (StatusCode::CONFLICT, "Share already finalized".to_string())
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/splits/participants/{id}/finalize",
    tag = "splits",
    params(("id" = String, Path, description = "split participant id")),
    request_body = crate::models::FinalizeSharePayload,
    responses(
        (status = 200, description = "Share finalized", body = crate::models::Record),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 404, description = "Share not found"),
        (status = 409, description = "Conflict"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn finalize_share(
    State(app_state): State<AppState>,
    session: Session,
    Path(participant_id): Path<String>,
    Json(payload): Json<FinalizeSharePayload>,
) -> Result<(StatusCode, Json<Record>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_string_length(
        &participant_id,
        "Participant ID",
        crate::constants::MAX_RECORD_NAME_LENGTH,
    )?;
    crate::records::validate_category_id(&payload.category_id)?;
    let category_id = payload.category_id.trim().to_string();

    let record = with_transaction(&app_state.main_db, |conn| {
        let current_user_id = current_user.id.clone();
        let participant_id = participant_id.clone();
        let category_id = category_id.clone();
        Box::pin(async move {
            let share = {
                let mut rows = conn
                    .query(
                        "SELECT sp.debtor_user_id, sp.amount, sp.finalized_record_id, s.description, s.currency, s.date \
                         FROM split_participants sp JOIN splits s ON s.id = sp.split_id WHERE sp.id = ?",
                        [participant_id.as_str()],
                    )
                    .await
                    .map_err(|_| FinalizeShareError::Db("failed to retrieve share"))?;

                let row = rows
                    .next()
                    .await
                    .map_err(|_| FinalizeShareError::Db("failed to retrieve share"))?
                    .ok_or(FinalizeShareError::NotFound)?;

                ShareToFinalize {
                    debtor_user_id: row
                        .get(0)
                        .map_err(|_| FinalizeShareError::Db("invalid share data"))?,
                    amount: row
                        .get(1)
                        .map_err(|_| FinalizeShareError::Db("invalid share data"))?,
                    finalized_record_id: row
                        .get(2)
                        .map_err(|_| FinalizeShareError::Db("invalid share data"))?,
                    description: row
                        .get(3)
                        .map_err(|_| FinalizeShareError::Db("invalid share data"))?,
                    currency: row
                        .get(4)
                        .map_err(|_| FinalizeShareError::Db("invalid share data"))?,
                    date: row
                        .get(5)
                        .map_err(|_| FinalizeShareError::Db("invalid share data"))?,
                }
            };

            if share.debtor_user_id != current_user_id {
                return Err(FinalizeShareError::NotFound);
            }

            if share.finalized_record_id.is_some() {
                return Err(FinalizeShareError::Conflict);
            }

            let is_income = {
                let mut rows = conn
                    .query(
                        "SELECT is_income FROM categories WHERE id = ? AND owner_user_id = ?",
                        (category_id.as_str(), current_user_id.as_str()),
                    )
                    .await
                    .map_err(|_| FinalizeShareError::Db("failed to check category"))?;

                rows.next()
                    .await
                    .map_err(|_| FinalizeShareError::Db("failed to check category"))?
                    .ok_or(FinalizeShareError::CategoryNotFound)?
                    .get::<bool>(0)
                    .map_err(|_| FinalizeShareError::Db("invalid category data"))?
            };

            let amount_cents = if is_income {
                share.amount.abs()
            } else {
                -share.amount.abs()
            };
            let record_id = Uuid::new_v4().to_string();

            conn.execute(
                "INSERT INTO records (id, owner_user_id, name, amount, currency, category_id, date) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    record_id.as_str(),
                    current_user_id.as_str(),
                    share.description.as_str(),
                    amount_cents,
                    share.currency.as_str(),
                    category_id.as_str(),
                    share.date.as_str(),
                ),
            )
            .await
            .map_err(|_| FinalizeShareError::Db("record creation failed"))?;

            let affected = conn
                .execute(
                    "UPDATE split_participants SET finalized_record_id = ? WHERE id = ? AND finalized_record_id IS NULL",
                    (record_id.as_str(), participant_id.as_str()),
                )
                .await
                .map_err(|_| FinalizeShareError::Db("share finalization failed"))?;

            if affected != 1 {
                return Err(FinalizeShareError::Conflict);
            }

            Ok(Record {
                id: record_id,
                name: share.description,
                amount: to_decimal(amount_cents),
                currency: share.currency,
                category_id: Some(category_id),
                date: share.date,
            })
        })
    })
    .await
    .map_err(Into::<(StatusCode, String)>::into)?;

    Ok((StatusCode::OK, Json(record)))
}
