use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::constants::*;
use crate::errors::db_error;
use crate::models::{FriendshipRelation, PublicUser};

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

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;
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

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    // pending=true  → incoming only (requester_user_id != current user)
    // pending=false or omitted → accepted friends (pending = 0)
    let show_pending_incoming = query.pending.unwrap_or(false);

    let total_count: i64 = if show_pending_incoming {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship WHERE (user_low_id = ? OR user_high_id = ?) AND pending = 1 AND requester_user_id != ?",
                (user_id.as_str(), user_id.as_str(), user_id.as_str()),
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
                "SELECT COUNT(*) FROM friendship WHERE (user_low_id = ? OR user_high_id = ?) AND pending = 0",
                (user_id.as_str(), user_id.as_str()),
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

    let mut rows = if show_pending_incoming {
        conn.query(
            "SELECT f.id, CASE WHEN f.user_low_id = ? THEN f.user_high_id ELSE f.user_low_id END AS user_id, f.pending, COALESCE(n.nickname, u.name) AS nickname
             FROM friendship f
             JOIN users u ON u.id = CASE WHEN f.user_low_id = ? THEN f.user_high_id ELSE f.user_low_id END
             LEFT JOIN friendship_nicknames n ON n.friendship_id = f.id AND n.owner_user_id = ?
             WHERE (f.user_low_id = ? OR f.user_high_id = ?) AND f.pending = 1 AND f.requester_user_id != ?
             ORDER BY nickname LIMIT ? OFFSET ?",
            (
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                limit,
                offset,
            ),
        )
        .await
    } else {
        conn.query(
            "SELECT f.id, CASE WHEN f.user_low_id = ? THEN f.user_high_id ELSE f.user_low_id END AS user_id, f.pending, COALESCE(n.nickname, u.name) AS nickname
             FROM friendship f
             JOIN users u ON u.id = CASE WHEN f.user_low_id = ? THEN f.user_high_id ELSE f.user_low_id END
             LEFT JOIN friendship_nicknames n ON n.friendship_id = f.id AND n.owner_user_id = ?
             WHERE (f.user_low_id = ? OR f.user_high_id = ?) AND f.pending = 0
             ORDER BY nickname LIMIT ? OFFSET ?",
            (
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                user_id.as_str(),
                limit,
                offset,
            ),
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
        let nickname: String = row
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
