use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::constants::*;
use crate::errors::db_error_with_context;
use crate::models::{FriendshipRelation, UpdateNicknamePayload};
use crate::{TransactionError, with_transaction};

use super::ordered_user_pair;

enum UpdateNicknameError {
    Transaction(TransactionError),
    DbCheck,
    DbUpdate,
    DbSelect,
    NotFound,
}

impl From<TransactionError> for UpdateNicknameError {
    fn from(e: TransactionError) -> Self {
        Self::Transaction(e)
    }
}

impl From<UpdateNicknameError> for (StatusCode, String) {
    fn from(e: UpdateNicknameError) -> Self {
        match e {
            UpdateNicknameError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            UpdateNicknameError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            UpdateNicknameError::DbCheck => {
                db_error_with_context("failed to check friendship relation")
            }
            UpdateNicknameError::DbUpdate => db_error_with_context("nickname update failed"),
            UpdateNicknameError::DbSelect => {
                db_error_with_context("failed to retrieve updated relation")
            }
            UpdateNicknameError::NotFound => (
                StatusCode::NOT_FOUND,
                "Friendship relation not found".to_string(),
            ),
        }
    }
}

#[utoipa::path(
    patch,
    path = "/friends/nickname",
    tag = "friends",
    request_body = crate::models::UpdateNicknamePayload,
    responses(
        (status = 200, description = "Nickname updated", body = crate::models::FriendshipRelation),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 404, description = "Friendship relation not found"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn update_nickname(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<UpdateNicknamePayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = &current_user.id;

    if let Some(ref nickname) = payload.nickname {
        if nickname.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Nickname cannot be empty string (use null to remove)".to_string(),
            ));
        }

        if nickname.len() > MAX_NICKNAME_LENGTH {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Nickname cannot exceed {} characters", MAX_NICKNAME_LENGTH),
            ));
        }
    }

    let (user_low_id, user_high_id) = ordered_user_pair(user_id, &payload.friend_id);

    let relation = with_transaction(&app_state.main_db, |conn| {
        let owner_user_id = user_id.clone();
        let friend_id = payload.friend_id.clone();
        let nickname = payload.nickname.clone();
        let user_low_id = user_low_id.to_string();
        let user_high_id = user_high_id.to_string();
        Box::pin(async move {
            let mut rows = conn
                .query(
                    "SELECT id FROM friendship WHERE user_low_id = ? AND user_high_id = ?",
                    (user_low_id.as_str(), user_high_id.as_str()),
                )
                .await
                .map_err(|_| UpdateNicknameError::DbCheck)?;

            let friendship_id: String = rows
                .next()
                .await
                .map_err(|_| UpdateNicknameError::DbCheck)?
                .ok_or(UpdateNicknameError::NotFound)?
                .get(0)
                .map_err(|_| UpdateNicknameError::DbCheck)?;
            drop(rows);

            if let Some(nickname) = nickname {
                conn.execute(
                    "INSERT INTO friendship_nicknames (friendship_id, owner_user_id, nickname) VALUES (?, ?, ?)
                     ON CONFLICT(friendship_id, owner_user_id) DO UPDATE SET nickname = excluded.nickname",
                    (friendship_id.as_str(), owner_user_id.as_str(), nickname.as_str()),
                )
                .await
                .map_err(|_| UpdateNicknameError::DbUpdate)?;
            } else {
                conn.execute(
                    "DELETE FROM friendship_nicknames WHERE friendship_id = ? AND owner_user_id = ?",
                    (friendship_id.as_str(), owner_user_id.as_str()),
                )
                .await
                .map_err(|_| UpdateNicknameError::DbUpdate)?;
            }

            let mut rows = conn
                .query(
                    "SELECT f.id, f.pending, COALESCE(n.nickname, u.name) AS nickname
                     FROM friendship f
                     JOIN users u ON u.id = ?
                     LEFT JOIN friendship_nicknames n ON n.friendship_id = f.id AND n.owner_user_id = ?
                     WHERE f.id = ?",
                    (friend_id.as_str(), owner_user_id.as_str(), friendship_id.as_str()),
                )
                .await
                .map_err(|_| UpdateNicknameError::DbSelect)?;

            let row = rows
                .next()
                .await
                .map_err(|_| UpdateNicknameError::DbSelect)?
                .ok_or(UpdateNicknameError::DbSelect)?;

            let id: String = row.get(0).map_err(|_| UpdateNicknameError::DbSelect)?;
            let pending: i64 = row.get(1).map_err(|_| UpdateNicknameError::DbSelect)?;
            let nickname: String = row.get(2).map_err(|_| UpdateNicknameError::DbSelect)?;

            Ok::<FriendshipRelation, UpdateNicknameError>(FriendshipRelation {
                id,
                user_id: friend_id,
                pending: pending != 0,
                nickname,
            })
        })
    })
    .await
    .map_err(Into::<(StatusCode, String)>::into)?;

    Ok((StatusCode::OK, Json(relation)))
}
