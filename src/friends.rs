use axum::extract::Query;
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::json;
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{get_current_user, get_user_by_username_public};
use crate::constants::*;
use crate::models::{
    AcceptFriendPayload, FriendshipRelation, PublicUser, RemoveFriendPayload,
    SendFriendRequestPayload, UpdateNicknamePayload,
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

    let conn = app_state.main_db.write().await;

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
        user_id: friend_user.id,
        pending: true,
        nickname: None,
    };

    Ok((StatusCode::CREATED, Json(relation)))
}

#[derive(Deserialize)]
pub struct SearchUsersQuery {
    pub query: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn search_users(
    State(app_state): State<AppState>,
    session: Session,
    Query(params): Query<SearchUsersQuery>,
) -> Result<(StatusCode, Json<Vec<PublicUser>>), (StatusCode, String)> {
    let _current_user = get_current_user(&session).await?;

    if params.query.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Query cannot be empty".to_string()));
    }

    if params.query.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Query must be at least 3 characters long".to_string(),
        ));
    }

    if params.query.len() > MAX_SEARCH_TERM_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Query cannot exceed {} characters", MAX_SEARCH_TERM_LENGTH),
        ));
    }

    let limit = params.limit.unwrap_or(20).min(MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).min(MAX_OFFSET);

    if limit == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Limit must be at least 1".to_string(),
        ));
    }

    let search_pattern = format!("{}%", params.query);

    let conn = app_state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name FROM users WHERE name LIKE ? LIMIT ? OFFSET ?",
            (search_pattern.as_str(), limit, offset),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut users = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        let id: String = row
            .get(0)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let username: String = row
            .get(1)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        users.push(PublicUser { id, username });
    }

    Ok((StatusCode::OK, Json(users)))
}

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

    let conn = app_state.main_db.read().await;
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

    let conn = app_state.main_db.write().await;

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
            "SELECT id, to_user_id as user_id, pending, nickname FROM friendship WHERE from_user_id = ? AND to_user_id = ?",
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
        let nickname: Option<String> = row
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

#[derive(Deserialize)]
pub struct ListFriendsQuery {
    pub pending: Option<bool>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn list_friends(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<ListFriendsQuery>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = &current_user.id;

    let limit = query.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).min(MAX_OFFSET);

    let conn = app_state.main_db.read().await;

    let pending_filter = query.pending.map(|value| if value { 1i64 } else { 0i64 });

    let total_count: i64 = if let Some(pending_value) = pending_filter {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship WHERE from_user_id = ? AND pending = ?",
                (user_id.as_str(), pending_value),
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            row.get(0)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        } else {
            0
        }
    } else {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship WHERE from_user_id = ?",
                [user_id.as_str()],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            row.get(0)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        } else {
            0
        }
    };

    let mut rows = if let Some(pending_value) = pending_filter {
        conn.query(
            "SELECT id, to_user_id as user_id, pending, nickname FROM friendship WHERE from_user_id = ? AND pending = ? ORDER BY nickname LIMIT ? OFFSET ?",
            (user_id.as_str(), pending_value, limit, offset),
        )
        .await
    } else {
        conn.query(
            "SELECT id, to_user_id as user_id, pending, nickname FROM friendship WHERE from_user_id = ? ORDER BY nickname LIMIT ? OFFSET ?",
            (user_id.as_str(), limit, offset),
        )
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut friends = Vec::new();
    while let Some(row) = rows
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
        let nickname: Option<String> = row
            .get(3)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        friends.push(FriendshipRelation {
            id,
            user_id: user_id_field,
            pending: pending_val != 0,
            nickname,
        });
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "friends": friends,
            "total_count": total_count,
            "limit": limit,
            "offset": offset
        })),
    ))
}

pub async fn accept_friend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<AcceptFriendPayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = &current_user.id;

    let conn = app_state.main_db.read().await;

    let mut rows = conn
        .query(
            "SELECT id, from_user_id, to_user_id, pending, nickname, requester_user_id FROM friendship WHERE from_user_id = ? AND to_user_id = ?",
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
    let nickname: Option<String> = row
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

    let conn = app_state.main_db.write().await;

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

    let conn = app_state.main_db.read().await;
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
        return Err((
            StatusCode::NOT_FOUND,
            "Friendship not found".to_string(),
        ));
    }

    drop(rows);
    drop(conn);

    let conn = app_state.main_db.write().await;
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

    Ok((StatusCode::OK, Json(json!({})) ))
}
