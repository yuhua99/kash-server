use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{get_current_user, get_user_by_username_public};
use crate::constants::*;
use crate::errors::db_error_with_context;
use crate::models::{
    AcceptFriendPayload, FriendshipRelation, RemoveFriendPayload, RemoveFriendResponse,
    SendFriendRequestPayload,
};
use crate::{TransactionError, with_transaction};

use super::ordered_user_pair;

enum SendFriendRequestError {
    Transaction(TransactionError),
    DbCheck,
    DbInsert,
    Conflict,
}

impl From<TransactionError> for SendFriendRequestError {
    fn from(e: TransactionError) -> Self {
        Self::Transaction(e)
    }
}

impl From<SendFriendRequestError> for (StatusCode, String) {
    fn from(e: SendFriendRequestError) -> Self {
        match e {
            SendFriendRequestError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            SendFriendRequestError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            SendFriendRequestError::DbCheck => {
                db_error_with_context("failed to check existing friendship")
            }
            SendFriendRequestError::DbInsert => {
                db_error_with_context("friend request creation failed")
            }
            SendFriendRequestError::Conflict => (
                StatusCode::CONFLICT,
                "Friend request already exists".to_string(),
            ),
        }
    }
}

enum AcceptFriendError {
    Transaction(TransactionError),
    DbUpdate,
    DbSelect,
    NotFound,
}

impl From<TransactionError> for AcceptFriendError {
    fn from(e: TransactionError) -> Self {
        Self::Transaction(e)
    }
}

impl From<AcceptFriendError> for (StatusCode, String) {
    fn from(e: AcceptFriendError) -> Self {
        match e {
            AcceptFriendError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            AcceptFriendError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            AcceptFriendError::DbUpdate => {
                db_error_with_context("friend request acceptance failed")
            }
            AcceptFriendError::DbSelect => {
                db_error_with_context("failed to retrieve friendship relation")
            }
            AcceptFriendError::NotFound => (
                StatusCode::NOT_FOUND,
                "Friend request not found".to_string(),
            ),
        }
    }
}

enum RemoveFriendError {
    Transaction(TransactionError),
    DbDelete,
    NotFound,
}

impl From<TransactionError> for RemoveFriendError {
    fn from(e: TransactionError) -> Self {
        Self::Transaction(e)
    }
}

impl From<RemoveFriendError> for (StatusCode, String) {
    fn from(e: RemoveFriendError) -> Self {
        match e {
            RemoveFriendError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            RemoveFriendError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            RemoveFriendError::DbDelete => db_error_with_context("friendship removal failed"),
            RemoveFriendError::NotFound => {
                (StatusCode::NOT_FOUND, "Friendship not found".to_string())
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/friends/request",
    tag = "friends",
    request_body = crate::models::SendFriendRequestPayload,
    responses(
        (status = 201, description = "Friend request created", body = crate::models::FriendshipRelation),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 404, description = "User not found"),
        (status = 409, description = "Friend request already exists"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn send_friend_request(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<SendFriendRequestPayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;

    if payload.friend_username.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Friend username cannot be empty".to_string(),
        ));
    }

    if payload.friend_username.len() > MAX_USERNAME_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Username cannot exceed {} characters", MAX_USERNAME_LENGTH),
        ));
    }

    if payload.friend_username == current_user.username {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot send friend request to yourself".to_string(),
        ));
    }

    let friend_user = get_user_by_username_public(&app_state.main_db, &payload.friend_username)
        .await
        .map_err(|_| db_error_with_context("failed to find user"))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "User not found".to_string()))?;

    let relation_id = Uuid::new_v4().to_string();
    let (user_low_id, user_high_id) = ordered_user_pair(&current_user.id, &friend_user.id);

    with_transaction(&app_state.main_db, |conn| {
        let relation_id = relation_id.clone();
        let current_user_id = current_user.id.clone();
        let user_low_id = user_low_id.to_string();
        let user_high_id = user_high_id.to_string();
        Box::pin(async move {
            let mut rows = conn
                .query(
                    "SELECT id FROM friendship WHERE user_low_id = ? AND user_high_id = ?",
                    (user_low_id.as_str(), user_high_id.as_str()),
                )
                .await
                .map_err(|_| SendFriendRequestError::DbCheck)?;

            if rows
                .next()
                .await
                .map_err(|_| SendFriendRequestError::DbCheck)?
                .is_some()
            {
                return Err(SendFriendRequestError::Conflict);
            }
            drop(rows);

            conn.execute(
                "INSERT INTO friendship (id, user_low_id, user_high_id, requester_user_id, pending) VALUES (?, ?, ?, ?, 1)",
                (
                    relation_id.as_str(),
                    user_low_id.as_str(),
                    user_high_id.as_str(),
                    current_user_id.as_str(),
                ),
            )
            .await
            .map_err(|_| SendFriendRequestError::DbInsert)?;

            Ok(())
        })
    })
    .await
    .map_err(Into::<(StatusCode, String)>::into)?;

    Ok((
        StatusCode::CREATED,
        Json(FriendshipRelation {
            id: relation_id,
            user_id: friend_user.id.clone(),
            pending: true,
            nickname: friend_user.username.clone(),
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/friends/accept",
    tag = "friends",
    request_body = crate::models::AcceptFriendPayload,
    responses(
        (status = 200, description = "Friend request accepted", body = crate::models::FriendshipRelation),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Friend request not found"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn accept_friend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<AcceptFriendPayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let (user_low_id, user_high_id) = ordered_user_pair(&current_user.id, &payload.friend_id);

    let relation = with_transaction(&app_state.main_db, |conn| {
        let current_user_id = current_user.id.clone();
        let friend_id = payload.friend_id.clone();
        let user_low_id = user_low_id.to_string();
        let user_high_id = user_high_id.to_string();
        Box::pin(async move {
            let changed = conn
                .execute(
                    "UPDATE friendship SET pending = 0 WHERE user_low_id = ? AND user_high_id = ? AND pending = 1 AND requester_user_id != ?",
                    (
                        user_low_id.as_str(),
                        user_high_id.as_str(),
                        current_user_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| AcceptFriendError::DbUpdate)?;

            if changed != 1 {
                return Err(AcceptFriendError::NotFound);
            }

            let mut rows = conn
                .query(
                    "SELECT f.id, f.pending, COALESCE(n.nickname, u.name) AS nickname
                     FROM friendship f
                     JOIN users u ON u.id = ?
                     LEFT JOIN friendship_nicknames n ON n.friendship_id = f.id AND n.owner_user_id = ?
                     WHERE f.user_low_id = ? AND f.user_high_id = ?",
                    (
                        friend_id.as_str(),
                        current_user_id.as_str(),
                        user_low_id.as_str(),
                        user_high_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| AcceptFriendError::DbSelect)?;

            let row = rows
                .next()
                .await
                .map_err(|_| AcceptFriendError::DbSelect)?
                .ok_or(AcceptFriendError::DbSelect)?;

            let id: String = row.get(0).map_err(|_| AcceptFriendError::DbSelect)?;
            let pending: i64 = row.get(1).map_err(|_| AcceptFriendError::DbSelect)?;
            let nickname: String = row.get(2).map_err(|_| AcceptFriendError::DbSelect)?;

            Ok(FriendshipRelation {
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

#[utoipa::path(
    post,
    path = "/friends/remove",
    tag = "friends",
    request_body = crate::models::RemoveFriendPayload,
    responses(
        (status = 200, description = "Friendship removed", body = crate::models::RemoveFriendResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Friendship not found"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn remove_friend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<RemoveFriendPayload>,
) -> Result<(StatusCode, Json<RemoveFriendResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let (user_low_id, user_high_id) = ordered_user_pair(&current_user.id, &payload.friend_id);

    with_transaction(&app_state.main_db, |conn| {
        let user_low_id = user_low_id.to_string();
        let user_high_id = user_high_id.to_string();
        Box::pin(async move {
            let changed = conn
                .execute(
                    "DELETE FROM friendship WHERE user_low_id = ? AND user_high_id = ?",
                    (user_low_id.as_str(), user_high_id.as_str()),
                )
                .await
                .map_err(|_| RemoveFriendError::DbDelete)?;

            if changed == 0 {
                return Err(RemoveFriendError::NotFound);
            }

            Ok(())
        })
    })
    .await
    .map_err(Into::<(StatusCode, String)>::into)?;

    Ok((StatusCode::OK, Json(RemoveFriendResponse {})))
}
