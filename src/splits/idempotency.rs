use axum::http::StatusCode;
use uuid::Uuid;

use crate::AppState;
use crate::errors::{db_error, db_error_with_context};
use crate::models::CreateSplitPayload;

const SPLIT_CREATE_ENDPOINT: &str = "/splits/create";
pub(super) const IDEMPOTENCY_TTL_HOURS: i64 = 24;
pub(super) const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 255;

pub(super) struct CachedIdempotency {
    pub response_status: i64,
    pub response_body: String,
    pub payload_hash: String,
}

pub(super) async fn get_existing_idempotency_response(
    app_state: &AppState,
    user_id: &str,
    idempotency_key: &str,
) -> Result<Option<CachedIdempotency>, (StatusCode, String)> {
    let maybe_cached = {
        let conn = app_state.main_db.connect().map_err(|_| db_error())?;
        let mut rows = conn
            .query(
                "SELECT response_status, response_body, payload_hash FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ?",
                (idempotency_key, user_id, SPLIT_CREATE_ENDPOINT),
            )
            .await
            .map_err(|_| db_error_with_context("failed to query idempotency key"))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|_| db_error_with_context("failed to read idempotency key row"))?
        {
            let response_status: i64 = row
                .get(0)
                .map_err(|_| db_error_with_context("invalid idempotency status"))?;
            let response_body: Option<String> = row
                .get(1)
                .map_err(|_| db_error_with_context("invalid idempotency response body"))?;
            let payload_hash: String = row
                .get(2)
                .map_err(|_| db_error_with_context("invalid idempotency payload hash"))?;
            Some((response_status, response_body, payload_hash))
        } else {
            None
        }
        // read lock dropped here
    };

    if let Some((response_status, response_body, payload_hash)) = maybe_cached {
        // A NULL response_body means a reservation was written but the fanout
        // never completed (e.g. the server crashed mid-write). Clear the stale
        // reservation so the caller can retry cleanly.
        let Some(response_body) = response_body else {
            let _ = delete_idempotency_reservation(app_state, idempotency_key, user_id).await;
            return Ok(None);
        };

        return Ok(Some(CachedIdempotency {
            response_status,
            response_body,
            payload_hash,
        }));
    }

    Ok(None)
}

pub(super) async fn reserve_idempotency_entry(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
    payload_hash: &str,
    created_at: &str,
    expires_at: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;
    conn.execute(
        "INSERT INTO idempotency_keys (id, key, user_id, endpoint, payload_hash, response_status, response_body, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?)",
        (
            Uuid::new_v4().to_string(),
            idempotency_key,
            user_id,
            SPLIT_CREATE_ENDPOINT,
            payload_hash,
            0i64,
            created_at,
            expires_at,
        ),
    )
    .await
    .map_err(|_| db_error_with_context("failed to reserve idempotency key"))?;

    Ok(())
}

pub(super) async fn commit_idempotency_entry(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
    response_status: i64,
    response_body: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;
    conn.execute(
        "UPDATE idempotency_keys SET response_status = ?, response_body = ? WHERE key = ? AND user_id = ? AND endpoint = ?",
        (
            response_status,
            response_body,
            idempotency_key,
            user_id,
            SPLIT_CREATE_ENDPOINT,
        ),
    )
    .await
    .map_err(|_| db_error_with_context("failed to commit idempotency entry"))?;

    Ok(())
}

pub(super) async fn delete_idempotency_reservation(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;
    conn.execute(
        "DELETE FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ? AND response_body IS NULL",
        (idempotency_key, user_id, SPLIT_CREATE_ENDPOINT),
    )
    .await
    .map_err(|_| db_error_with_context("failed to delete idempotency reservation"))?;

    Ok(())
}

pub(super) fn compute_payload_hash(
    payload: &CreateSplitPayload,
) -> Result<String, (StatusCode, String)> {
    let serialized = serde_json::to_string(payload).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize payload: {}", e),
        )
    })?;

    Ok(fnv1a_64_hex(serialized.as_bytes()))
}

fn fnv1a_64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

pub(super) fn now_rfc3339() -> Result<String, (StatusCode, String)> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
