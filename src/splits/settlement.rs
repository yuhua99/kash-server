use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::auth::get_current_user;
use crate::errors::db_error_with_context;
use crate::models::{SettleAllResponse, ShareStatusResponse};
use crate::validation::validate_string_length;
use crate::{AppState, TransactionError, with_transaction};

enum SettleShareError {
    Transaction(TransactionError),
    Db(&'static str),
    NotFound,
    Forbidden,
}

impl From<TransactionError> for SettleShareError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

#[utoipa::path(
    put,
    path = "/splits/participants/{id}/settle",
    tag = "splits",
    params(("id" = String, Path, description = "split participant id")),
    responses(
        (status = 200, description = "Share settled", body = crate::models::ShareStatusResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 404, description = "Share not found"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn settle_share(
    State(app_state): State<AppState>,
    session: Session,
    Path(participant_id): Path<String>,
) -> Result<(StatusCode, Json<ShareStatusResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_string_length(
        &participant_id,
        "Participant ID",
        crate::constants::MAX_RECORD_NAME_LENGTH,
    )?;

    let response = with_transaction(&app_state.main_db, |conn| {
        let participant_id = participant_id.clone();
        let current_user_id = current_user.id.clone();

        Box::pin(async move {
            let (debtor_user_id, settled, finalized, creditor_user_id) = {
                let mut rows = conn
                    .query(
                        "SELECT sp.debtor_user_id, sp.settled, (sp.finalized_record_id IS NOT NULL) AS finalized, s.creditor_user_id FROM split_participants sp JOIN splits s ON s.id = sp.split_id WHERE sp.id = ?",
                        [participant_id.as_str()],
                    )
                    .await
                    .map_err(|_| SettleShareError::Db("failed to load share"))?;

                let Some(row) = rows
                    .next()
                    .await
                    .map_err(|_| SettleShareError::Db("failed to load share"))?
                else {
                    return Err(SettleShareError::NotFound);
                };

                let debtor_user_id: String = row
                    .get(0)
                    .map_err(|_| SettleShareError::Db("invalid share data"))?;
                let settled: i64 = row
                    .get(1)
                    .map_err(|_| SettleShareError::Db("invalid share data"))?;
                let finalized: i64 = row
                    .get(2)
                    .map_err(|_| SettleShareError::Db("invalid share data"))?;
                let creditor_user_id: String = row
                    .get(3)
                    .map_err(|_| SettleShareError::Db("invalid share data"))?;

                (debtor_user_id, settled != 0, finalized != 0, creditor_user_id)
            };

            if current_user_id != debtor_user_id && current_user_id != creditor_user_id {
                return Err(SettleShareError::Forbidden);
            }

            if settled {
                return Ok(ShareStatusResponse {
                    participant_id,
                    settled: true,
                    finalized,
                });
            }

            let affected = conn
                .execute(
                    "UPDATE split_participants SET settled = 1 WHERE id = ? AND settled = 0",
                    [participant_id.as_str()],
                )
                .await
                .map_err(|_| SettleShareError::Db("failed to settle share"))?;

            if affected != 1 {
                return Err(SettleShareError::NotFound);
            }

            Ok(ShareStatusResponse {
                participant_id,
                settled: true,
                finalized,
            })
        })
    })
    .await
    .map_err(|e| match e {
        SettleShareError::NotFound | SettleShareError::Forbidden => {
            (StatusCode::NOT_FOUND, "Share not found".to_string())
        }
        SettleShareError::Transaction(TransactionError::Begin) => {
            db_error_with_context("failed to begin settle share transaction")
        }
        SettleShareError::Transaction(TransactionError::Commit) => {
            db_error_with_context("failed to commit settle share transaction")
        }
        SettleShareError::Db(context) => db_error_with_context(context),
    })?;

    Ok((StatusCode::OK, Json(response)))
}

#[utoipa::path(
    put,
    path = "/splits/with/{friend_id}/settle-all",
    tag = "splits",
    params(("friend_id" = String, Path, description = "friend user id")),
    responses(
        (status = 200, description = "Shares settled", body = crate::models::SettleAllResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn settle_all_with_friend(
    State(app_state): State<AppState>,
    session: Session,
    Path(friend_id): Path<String>,
) -> Result<(StatusCode, Json<SettleAllResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_string_length(
        &friend_id,
        "Friend ID",
        crate::constants::MAX_RECORD_NAME_LENGTH,
    )?;
    let friend_id = friend_id.trim().to_string();
    if friend_id == current_user.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Friend ID cannot be your own user ID".to_string(),
        ));
    }

    let updated_count = with_transaction(&app_state.main_db, |conn| {
        let current_user_id = current_user.id.clone();
        let friend_id = friend_id.clone();

        Box::pin(async move {
            let affected = conn
                .execute(
                    "UPDATE split_participants SET settled = 1 WHERE settled = 0 AND EXISTS (SELECT 1 FROM splits s WHERE s.id = split_participants.split_id AND ((split_participants.debtor_user_id = ? AND s.creditor_user_id = ?) OR (split_participants.debtor_user_id = ? AND s.creditor_user_id = ?)))",
                    (
                        current_user_id.as_str(),
                        friend_id.as_str(),
                        friend_id.as_str(),
                        current_user_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| TransactionError::Commit)?;

            u32::try_from(affected).map_err(|_| TransactionError::Commit)
        })
    })
    .await
    .map_err(|e| match e {
        TransactionError::Begin => db_error_with_context("failed to begin settle all transaction"),
        TransactionError::Commit => db_error_with_context("failed to commit settle all transaction"),
    })?;

    Ok((StatusCode::OK, Json(SettleAllResponse { updated_count })))
}
