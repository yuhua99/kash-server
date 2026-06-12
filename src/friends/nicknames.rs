use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::constants::*;
use crate::errors::db_error;
use crate::models::{FriendshipRelation, UpdateNicknamePayload};

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

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;
    let mut rows = conn
        .query(
            "SELECT id, to_user_id as user_id, pending, nickname FROM friendship WHERE from_user_id = ? AND to_user_id = ?",
            (user_id.as_str(), payload.friend_id.as_str()),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows
        .next()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err((
            StatusCode::NOT_FOUND,
            "Friendship relation not found".to_string(),
        ));
    }

    drop(rows);
    drop(conn);

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    conn.execute(
        "UPDATE friendship SET nickname = ? WHERE from_user_id = ? AND to_user_id = ?",
        (
            payload.nickname.as_deref(),
            user_id.as_str(),
            payload.friend_id.as_str(),
        ),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut rows = conn
        .query(
            "SELECT f.id, f.to_user_id as user_id, f.pending, COALESCE(f.nickname, u.name) as nickname FROM friendship f JOIN users u ON u.id = f.to_user_id WHERE f.from_user_id = ? AND f.to_user_id = ?",
            (user_id.as_str(), payload.friend_id.as_str()),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        let id: String = row
            .get(0)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let user_id_field: String = row
            .get(1)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let pending_val: i64 = row
            .get(2)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let nickname: String = row
            .get(3)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let relation = FriendshipRelation {
            id,
            user_id: user_id_field,
            pending: pending_val != 0,
            nickname,
        };

        return Ok((StatusCode::OK, Json(relation)));
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to retrieve updated relation".to_string(),
    ))
}
