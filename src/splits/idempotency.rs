use axum::http::StatusCode;
use uuid::Uuid;

use crate::AppState;
use crate::errors::{db_error, db_error_with_context};
use crate::models::CreateSplitPayload;

pub(super) const SPLIT_CREATE_ENDPOINT: &str = "/splits/create";
pub(super) const IDEMPOTENCY_TTL_HOURS: i64 = 24;
pub(super) const RESERVATION_STALE_SECONDS: i64 = 300;
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
        let conn = crate::database::db_conn(&app_state.main_db)
            .await
            .inspect_err(|e| tracing::error!("db connection failed: {e}"))
            .map_err(|_| db_error())?;
        let mut rows = conn
            .query(
                "SELECT id, response_status, response_body, payload_hash, expires_at, created_at FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ?",
                (idempotency_key, user_id, SPLIT_CREATE_ENDPOINT),
            )
            .await
            .inspect_err(|e| tracing::error!("failed to query idempotency key: {e}"))
            .map_err(|_| db_error_with_context("failed to query idempotency key"))?;

        if let Some(row) = rows
            .next()
            .await
            .inspect_err(|e| tracing::error!("failed to read idempotency key row: {e}"))
            .map_err(|_| db_error_with_context("failed to read idempotency key row"))?
        {
            let reservation_id: String = row
                .get(0)
                .inspect_err(|e| tracing::error!("invalid idempotency id: {e}"))
                .map_err(|_| db_error_with_context("invalid idempotency id"))?;
            let response_status: i64 = row
                .get(1)
                .inspect_err(|e| tracing::error!("invalid idempotency status: {e}"))
                .map_err(|_| db_error_with_context("invalid idempotency status"))?;
            let response_body: Option<String> = row
                .get(2)
                .inspect_err(|e| tracing::error!("invalid idempotency response body: {e}"))
                .map_err(|_| db_error_with_context("invalid idempotency response body"))?;
            let payload_hash: String = row
                .get(3)
                .inspect_err(|e| tracing::error!("invalid idempotency payload hash: {e}"))
                .map_err(|_| db_error_with_context("invalid idempotency payload hash"))?;
            let expires_at: String = row
                .get(4)
                .inspect_err(|e| tracing::error!("invalid idempotency expiration: {e}"))
                .map_err(|_| db_error_with_context("invalid idempotency expiration"))?;
            let created_at: String = row
                .get(5)
                .inspect_err(|e| tracing::error!("invalid idempotency creation time: {e}"))
                .map_err(|_| db_error_with_context("invalid idempotency creation time"))?;
            Some((
                reservation_id,
                response_status,
                response_body,
                payload_hash,
                expires_at,
                created_at,
            ))
        } else {
            None
        }
        // read lock dropped here
    };

    if let Some((
        reservation_id,
        response_status,
        response_body,
        payload_hash,
        expires_at,
        created_at,
    )) = maybe_cached
    {
        if expires_at <= now_rfc3339()? {
            delete_idempotency_entry(app_state, idempotency_key, user_id).await?;
            return Ok(None);
        }

        // A NULL response_body means a request is in flight. Only clear it if
        // the reservation is old enough to be treated as a crashed-server
        // cleanup case.
        let Some(response_body) = response_body else {
            let created_at = time::OffsetDateTime::parse(
                &created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| db_error_with_context("invalid idempotency creation time"))?;

            if time::OffsetDateTime::now_utc() - created_at
                > time::Duration::seconds(RESERVATION_STALE_SECONDS)
            {
                let _ = delete_idempotency_reservation(app_state, &reservation_id).await;
                return Ok(None);
            }

            return Err((
                StatusCode::CONFLICT,
                "A request with this idempotency key is already in progress".to_string(),
            ));
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
) -> Result<String, (StatusCode, String)> {
    let reservation_id = Uuid::new_v4().to_string();
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    conn.execute(
        "INSERT INTO idempotency_keys (id, key, user_id, endpoint, payload_hash, response_status, response_body, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?)",
        (
            reservation_id.as_str(),
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
    .inspect_err(|e| tracing::error!("failed to reserve idempotency key: {e}"))
    .map_err(|_| db_error_with_context("failed to reserve idempotency key"))?;

    Ok(reservation_id)
}

pub(super) async fn delete_idempotency_reservation(
    app_state: &AppState,
    reservation_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    conn.execute(
        "DELETE FROM idempotency_keys WHERE id = ? AND response_body IS NULL",
        [reservation_id],
    )
    .await
    .inspect_err(|e| tracing::error!("failed to delete idempotency reservation: {e}"))
    .map_err(|_| db_error_with_context("failed to delete idempotency reservation"))?;

    Ok(())
}

async fn delete_idempotency_entry(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    conn.execute(
        "DELETE FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ?",
        (idempotency_key, user_id, SPLIT_CREATE_ENDPOINT),
    )
    .await
    .inspect_err(|e| tracing::error!("failed to delete expired idempotency entry: {e}"))
    .map_err(|_| db_error_with_context("failed to delete expired idempotency entry"))?;

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
