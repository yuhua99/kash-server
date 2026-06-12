use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::json;
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{get_current_user, get_user_by_username_public};
use crate::constants::*;
use crate::errors::db_error;
use crate::models::{
    AcceptFriendPayload, FriendshipRelation, RemoveFriendPayload, SendFriendRequestPayload,
};

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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "User not found".to_string()))?;

    let a_to_b_id = Uuid::new_v4().to_string();
    let b_to_a_id = Uuid::new_v4().to_string();

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tx_result: Result<(), String> = async {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship WHERE from_user_id = ? AND to_user_id = ?",
                (current_user.id.as_str(), friend_user.id.as_str()),
            )
            .await
            .map_err(|e| e.to_string())?;

        if let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            let count: i64 = row.get(0).map_err(|e| e.to_string())?;
            if count > 0 {
                return Err("FRIENDSHIP_EXISTS".to_string());
            }
        }
        drop(rows);

        conn.execute(
            "INSERT INTO friendship (id, from_user_id, to_user_id, pending, nickname, requester_user_id) VALUES (?, ?, ?, ?, NULL, ?)",
            (
                a_to_b_id.as_str(),
                current_user.id.as_str(),
                friend_user.id.as_str(),
                1i64,
                current_user.id.as_str(),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO friendship (id, from_user_id, to_user_id, pending, nickname, requester_user_id) VALUES (?, ?, ?, ?, NULL, ?)",
            (
                b_to_a_id.as_str(),
                friend_user.id.as_str(),
                current_user.id.as_str(),
                1i64,
                current_user.id.as_str(),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }
    .await;

    match tx_result {
        Ok(_) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            drop(conn);
            if e == "FRIENDSHIP_EXISTS" {
                return Err((
                    StatusCode::CONFLICT,
                    "Friend request already exists".to_string(),
                ));
            }
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e));
        }
    }

    let relation = FriendshipRelation {
        id: a_to_b_id,
        user_id: friend_user.id.clone(),
        pending: true,
        nickname: friend_user.username.clone(),
    };

    Ok((StatusCode::CREATED, Json(relation)))
}

pub async fn accept_friend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<AcceptFriendPayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = &current_user.id;

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    let mut rows = conn
        .query(
            "SELECT f.id, f.from_user_id, f.to_user_id, f.pending, COALESCE(f.nickname, u.name) as nickname, f.requester_user_id FROM friendship f JOIN users u ON u.id = f.from_user_id WHERE f.from_user_id = ? AND f.to_user_id = ?",
            (payload.friend_id.as_str(), user_id.as_str()),
        )
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = rows
        .next()
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Friend request not found".to_string(),
            )
        })?;

    let relation_id: String = row
        .get(0)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let from_user_id: String = row
        .get(1)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let to_user_id: String = row
        .get(2)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let pending_val: i64 = row
        .get(3)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let nickname: String = row
        .get(4)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let requester_user_id: String = row
        .get(5)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(rows);
    drop(conn);

    if pending_val == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Friend request not found".to_string(),
        ));
    }

    if user_id == &requester_user_id {
        return Err((
            StatusCode::NOT_FOUND,
            "Friend request not found".to_string(),
        ));
    }

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tx_result: Result<(), libsql::Error> = async {
        conn.execute(
            "UPDATE friendship SET pending = 0 WHERE from_user_id = ? AND to_user_id = ?",
            (from_user_id.as_str(), to_user_id.as_str()),
        )
        .await?;

        conn.execute(
            "UPDATE friendship SET pending = 0 WHERE from_user_id = ? AND to_user_id = ?",
            (to_user_id.as_str(), from_user_id.as_str()),
        )
        .await?;

        Ok(())
    }
    .await;

    match tx_result {
        Ok(_) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            drop(conn);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    }

    Ok((
        StatusCode::OK,
        Json(FriendshipRelation {
            id: relation_id,
            user_id: from_user_id,
            pending: false,
            nickname,
        }),
    ))
}

pub async fn remove_friend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<RemoveFriendPayload>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM friendship WHERE (from_user_id = ? AND to_user_id = ?) OR (from_user_id = ? AND to_user_id = ?)",
            (
                current_user.id.as_str(),
                payload.friend_id.as_str(),
                payload.friend_id.as_str(),
                current_user.id.as_str(),
            ),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let count = if let Some(row) = rows
        .next()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        row.get(0)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        0
    };

    if count == 0 {
        return Err((StatusCode::NOT_FOUND, "Friendship not found".to_string()));
    }

    drop(rows);
    drop(conn);

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;
    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tx_result: Result<(), libsql::Error> = async {
        conn.execute(
            "DELETE FROM friendship WHERE (from_user_id = ? AND to_user_id = ?) OR (from_user_id = ? AND to_user_id = ?)",
            (
                current_user.id.as_str(),
                payload.friend_id.as_str(),
                payload.friend_id.as_str(),
                current_user.id.as_str(),
            ),
        )
        .await?;

        Ok(())
    }
    .await;

    match tx_result {
        Ok(_) => {
            conn.execute("COMMIT", ())
                .await
                .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", ()).await;
            drop(conn);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    }

    Ok((StatusCode::OK, Json(json!({}))))
}
