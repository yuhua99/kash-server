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
    AcceptFriendPayload, BlockFriendPayload, FriendshipRelation, PublicUser,
    SendFriendRequestPayload, UnfriendPayload, UpdateNicknamePayload,
};
use crate::utils::validate_friendship_transition;

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

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let a_to_b_id = Uuid::new_v4().to_string();
    let b_to_a_id = Uuid::new_v4().to_string();

    let conn = app_state.main_db.write().await;

    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Inner result to signal friendship exists vs other DB errors
    let tx_result: Result<(), String> = async {
        // Check for existing friendship inside transaction
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
                (current_user.id.as_str(), friend_user.id.as_str()),
            )
            .await
            .map_err(|e| e.to_string())?;

        if let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            let count: i64 = row.get(0).map_err(|e| e.to_string())?;
            if count > 0 {
                // Friendship already exists, return special signal
                return Err("FRIENDSHIP_EXISTS".to_string());
            }
        }

        // Insert both relations
        conn.execute(
            "INSERT INTO friendship_relations (id, from_user_id, to_user_id, status, nickname, requester_user_id, requested_at, updated_at) VALUES (?, ?, ?, ?, NULL, ?, ?, ?)",
            (
                a_to_b_id.as_str(),
                current_user.id.as_str(),
                friend_user.id.as_str(),
                FRIEND_STATUS_PENDING,
                current_user.id.as_str(),
                now.as_str(),
                now.as_str(),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO friendship_relations (id, from_user_id, to_user_id, status, nickname, requester_user_id, requested_at, updated_at) VALUES (?, ?, ?, ?, NULL, ?, ?, ?)",
            (
                b_to_a_id.as_str(),
                friend_user.id.as_str(),
                current_user.id.as_str(),
                FRIEND_STATUS_PENDING,
                current_user.id.as_str(),
                now.as_str(),
                now.as_str(),
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
            // Check if this was our "friendship exists" signal or a real DB error
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
        status: FRIEND_STATUS_PENDING.to_string(),
        nickname: None,
        requested_at: now.clone(),
        updated_at: now,
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
            "SELECT id, to_user_id as user_id, status, nickname, requested_at, updated_at FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
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

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let conn = app_state.main_db.write().await;

    conn.execute(
        "UPDATE friendship_relations SET nickname = ?, updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
        (
            payload.nickname.as_deref(),
            now.as_str(),
            user_id.as_str(),
            payload.friend_id.as_str(),
        ),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut rows = conn
        .query(
            "SELECT id, to_user_id as user_id, status, nickname, requested_at, updated_at FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
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
        let status: String = row
            .get(2)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let nickname: Option<String> = row
            .get(3)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let requested_at: String = row
            .get(4)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let updated_at: String = row
            .get(5)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let relation = FriendshipRelation {
            id,
            user_id: user_id_field,
            status,
            nickname,
            requested_at,
            updated_at,
        };

        Ok((StatusCode::OK, Json(relation)))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to retrieve updated relation".to_string(),
        ))
    }
}

#[derive(Deserialize)]
pub struct ListFriendsQuery {
    pub status: Option<String>,
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

    if let Some(ref status) = query.status {
        match status.as_str() {
            FRIEND_STATUS_PENDING
            | FRIEND_STATUS_ACCEPTED
            | FRIEND_STATUS_BLOCKED
            | FRIEND_STATUS_UNFRIENDED => {}
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Invalid status: {}", status),
                ));
            }
        }
    }

    let limit = query.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).min(MAX_OFFSET);

    let conn = app_state.main_db.read().await;

    let total_count: i64 = if let Some(ref status) = query.status {
        let mut count_rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship_relations WHERE from_user_id = ? AND status = ?",
                (user_id.as_str(), status.as_str()),
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = count_rows
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
        let mut count_rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship_relations WHERE from_user_id = ?",
                [user_id.as_str()],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = count_rows
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

    let mut rows = if let Some(ref status) = query.status {
        conn.query(
            "SELECT id, to_user_id as user_id, status, nickname, requested_at, updated_at FROM friendship_relations WHERE from_user_id = ? AND status = ? ORDER BY updated_at DESC LIMIT ? OFFSET ?",
            (user_id.as_str(), status.as_str(), limit, offset),
        )
        .await
    } else {
        conn.query(
            "SELECT id, to_user_id as user_id, status, nickname, requested_at, updated_at FROM friendship_relations WHERE from_user_id = ? ORDER BY updated_at DESC LIMIT ? OFFSET ?",
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
        let status: String = row
            .get(2)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let nickname: Option<String> = row
            .get(3)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let requested_at: String = row
            .get(4)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let updated_at: String = row
            .get(5)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        friends.push(FriendshipRelation {
            id,
            user_id: user_id_field,
            status,
            nickname,
            requested_at,
            updated_at,
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
            "SELECT id, from_user_id, to_user_id, status, nickname, requester_user_id, requested_at, updated_at FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
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
    let current_status: String = row
        .get(3)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let nickname: Option<String> = row
        .get(4)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let requester_user_id: String = row
        .get(5)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let requested_at: String = row
        .get(6)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(rows);
    drop(conn);

    validate_friendship_transition(&current_status, FRIEND_STATUS_ACCEPTED)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Authorization: Only the recipient (non-requester) can accept
    if user_id == &requester_user_id {
        return Err((
            StatusCode::NOT_FOUND,
            "Friend request not found".to_string(),
        ));
    }

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let conn = app_state.main_db.write().await;

    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tx_result: Result<(), libsql::Error> = async {
        conn.execute(
            "UPDATE friendship_relations SET status = ?, updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (FRIEND_STATUS_ACCEPTED, now.as_str(), from_user_id.as_str(), to_user_id.as_str()),
        )
        .await?;

        conn.execute(
            "UPDATE friendship_relations SET status = ?, updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (FRIEND_STATUS_ACCEPTED, now.as_str(), to_user_id.as_str(), from_user_id.as_str()),
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

    let relation = FriendshipRelation {
        id: relation_id,
        user_id: from_user_id,
        status: FRIEND_STATUS_ACCEPTED.to_string(),
        nickname,
        requested_at,
        updated_at: now,
    };

    Ok((StatusCode::OK, Json(relation)))
}

pub async fn block_friend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<BlockFriendPayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = &current_user.id;

    let conn = app_state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, status, nickname, requested_at FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
            (user_id.as_str(), payload.friend_id.as_str()),
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
    let current_status: String = row
        .get(1)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let nickname: Option<String> = row
        .get(2)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let requested_at: String = row
        .get(3)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(rows);
    drop(conn);

    validate_friendship_transition(&current_status, FRIEND_STATUS_BLOCKED)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let conn = app_state.main_db.write().await;

    conn.execute(
        "UPDATE friendship_relations SET status = ?, updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
        (FRIEND_STATUS_BLOCKED, now.as_str(), user_id.as_str(), payload.friend_id.as_str()),
    )
    .await
    .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let relation = FriendshipRelation {
        id: relation_id,
        user_id: payload.friend_id,
        status: FRIEND_STATUS_BLOCKED.to_string(),
        nickname,
        requested_at,
        updated_at: now,
    };

    Ok((StatusCode::OK, Json(relation)))
}

pub async fn unfriend(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<UnfriendPayload>,
) -> Result<(StatusCode, Json<FriendshipRelation>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let user_id = &current_user.id;

    let conn = app_state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, from_user_id, to_user_id, status, nickname, requested_at FROM friendship_relations WHERE (from_user_id = ? AND to_user_id = ?) OR (from_user_id = ? AND to_user_id = ?)",
            (user_id.as_str(), payload.friend_id.as_str(), payload.friend_id.as_str(), user_id.as_str()),
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
                "Friendship relation not found".to_string(),
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
    let current_status: String = row
        .get(3)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let nickname: Option<String> = row
        .get(4)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let requested_at: String = row
        .get(5)
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    drop(rows);
    drop(conn);

    validate_friendship_transition(&current_status, FRIEND_STATUS_UNFRIENDED)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let conn = app_state.main_db.write().await;

    conn.execute("BEGIN TRANSACTION", ())
        .await
        .map_err(|e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tx_result: Result<(), libsql::Error> = async {
        conn.execute(
            "UPDATE friendship_relations SET status = ?, updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (FRIEND_STATUS_UNFRIENDED, now.as_str(), from_user_id.as_str(), to_user_id.as_str()),
        )
        .await?;

        conn.execute(
            "UPDATE friendship_relations SET status = ?, updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (FRIEND_STATUS_UNFRIENDED, now.as_str(), to_user_id.as_str(), from_user_id.as_str()),
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
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    }

    let relation = FriendshipRelation {
        id: relation_id,
        user_id: payload.friend_id,
        status: FRIEND_STATUS_UNFRIENDED.to_string(),
        nickname,
        requested_at,
        updated_at: now,
    };

    Ok((StatusCode::OK, Json(relation)))
}
