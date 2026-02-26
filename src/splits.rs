use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::constants::*;
use crate::models::{
    CreateSplitPayload, PendingSplitsQuery, SplitListItem, SplitListResponse, SplitParticipant,
    UnsettledSplitsQuery,
};
use crate::utils::{
    calculate_split_amounts, db_error, db_error_with_context, validate_category_exists,
    validate_date, validate_offset, validate_records_limit, validate_split_participants,
    validate_string_length,
};
use crate::{AppState, TransactionError, with_transaction};

const SPLIT_CREATE_ENDPOINT: &str = "/splits/create";
const IDEMPOTENCY_TTL_HOURS: i64 = 24;
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 255;

enum SplitRecordError {
    Transaction,
    Db,
}

impl From<TransactionError> for SplitRecordError {
    fn from(_: TransactionError) -> Self {
        Self::Transaction
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateSplitResponse {
    pub split_id: String,
    pub payer_record_id: String,
    pub pending_record_ids: Vec<String>,
}

struct CachedIdempotency {
    response_status: i64,
    response_body: String,
    payload_hash: String,
}

pub async fn create_split(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateSplitPayload>,
) -> Result<(StatusCode, Json<CreateSplitResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_split_create_payload(&payload, &current_user.id)?;
    validate_all_participants_are_friends(&app_state, &current_user.id, &payload.splits).await?;

    let payload_hash = compute_payload_hash(&payload)?;
    if let Some(cached) =
        get_existing_idempotency_response(&app_state, &current_user.id, &payload.idempotency_key)
            .await?
    {
        if cached.payload_hash != payload_hash {
            return Err((
                StatusCode::CONFLICT,
                "Idempotency key already used with different payload".to_string(),
            ));
        }

        let response =
            serde_json::from_str::<CreateSplitResponse>(&cached.response_body).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to deserialize idempotency response".to_string(),
                )
            })?;

        let status = StatusCode::from_u16(cached.response_status as u16).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid cached response status".to_string(),
            )
        })?;

        return Ok((status, Json(response)));
    }

    let split_id = Uuid::new_v4().to_string();
    let now = now_rfc3339()?;
    let expires_at = (time::OffsetDateTime::now_utc()
        + time::Duration::hours(IDEMPOTENCY_TTL_HOURS))
    .format(&time::format_description::well_known::Rfc3339)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Reserve the idempotency key before writing any records. This ensures that
    // if the fanout partially succeeds and then fails, a client retry with the
    // same key will see the reservation (response_body = NULL) and get a 500
    // rather than re-running the fanout and creating duplicate records.
    reserve_idempotency_entry(
        &app_state,
        &payload.idempotency_key,
        &current_user.id,
        &payload_hash,
        &now,
        &expires_at,
    )
    .await?;

    let fanout_result =
        create_split_records(&app_state, &current_user.id, &split_id, &payload).await;

    let (payer_record_id, pending_record_ids) = match fanout_result {
        Ok(ids) => ids,
        Err(e) => {
            // Fanout failed — delete the reservation so the client can retry
            // cleanly with the same idempotency key.
            let _ = delete_idempotency_reservation(
                &app_state,
                &payload.idempotency_key,
                &current_user.id,
            )
            .await;
            return Err(e);
        }
    };

    let response = CreateSplitResponse {
        split_id,
        payer_record_id,
        pending_record_ids,
    };

    let response_body = serde_json::to_string(&response).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize response: {}", e),
        )
    })?;

    // Commit the response body into the reservation. If this fails, records are
    // already written — we still return 201; the key just won't deduplicate a
    // future retry (acceptable: rare, and the payer will see their records).
    let _ = commit_idempotency_entry(
        &app_state,
        &payload.idempotency_key,
        &current_user.id,
        i64::from(StatusCode::CREATED.as_u16()),
        &response_body,
    )
    .await;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn list_pending_splits(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<PendingSplitsQuery>,
) -> Result<(StatusCode, Json<SplitListResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;

    let conn = app_state.main_db.read().await;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND pending = 1 AND split_id IS NOT NULL",
            [current_user.id.as_str()],
        )
        .await
        .map_err(|_| db_error_with_context("failed to count pending splits"))?;

    let total_count: u32 = if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
        let raw_count: i64 = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid pending split count"))?;
        u32::try_from(raw_count)
            .map_err(|_| db_error_with_context("pending split count exceeds u32"))?
    } else {
        0
    };

    let mut rows = conn
        .query(
            "SELECT r.id, r.split_id, r.name, r.date, r.amount, r.debtor_user_id, r.creditor_user_id, COALESCE(creditor_user.name, ''), COALESCE(debtor_user.name, ''), r.pending, r.settle FROM records r LEFT JOIN users creditor_user ON creditor_user.id = r.creditor_user_id LEFT JOIN users debtor_user ON debtor_user.id = r.debtor_user_id WHERE r.owner_user_id = ? AND r.pending = 1 AND r.split_id IS NOT NULL ORDER BY r.date DESC, r.id DESC LIMIT ? OFFSET ?",
            (current_user.id.as_str(), limit, offset),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query pending splits"))?;

    let mut splits = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| db_error())? {
        splits.push(split_list_item_from_row(row, &current_user.id)?);
    }

    Ok((
        StatusCode::OK,
        Json(SplitListResponse {
            splits,
            total_count,
            limit,
            offset,
        }),
    ))
}

pub async fn list_unsettled_splits_with_friend(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<UnsettledSplitsQuery>,
) -> Result<(StatusCode, Json<SplitListResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;

    validate_string_length(&query.friend_id, "Friend ID", MAX_RECORD_NAME_LENGTH)?;
    let friend_id = query.friend_id.trim().to_string();
    if friend_id == current_user.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Friend ID cannot be your own user ID".to_string(),
        ));
    }

    validate_friend_is_accepted(&app_state, &current_user.id, &friend_id).await?;

    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;

    let conn = app_state.main_db.read().await;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND pending = 0 AND settle = 0 AND split_id IS NOT NULL AND ((debtor_user_id = ? AND creditor_user_id = ?) OR (debtor_user_id = ? AND creditor_user_id = ?))",
            (
                current_user.id.as_str(),
                current_user.id.as_str(),
                friend_id.as_str(),
                friend_id.as_str(),
                current_user.id.as_str(),
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to count unsettled splits"))?;

    let total_count: u32 = if let Some(row) = count_rows.next().await.map_err(|_| db_error())? {
        let raw_count: i64 = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid unsettled split count"))?;
        u32::try_from(raw_count)
            .map_err(|_| db_error_with_context("unsettled split count exceeds u32"))?
    } else {
        0
    };

    let mut rows = conn
        .query(
            "SELECT r.id, r.split_id, r.name, r.date, r.amount, r.debtor_user_id, r.creditor_user_id, COALESCE(creditor_user.name, ''), COALESCE(debtor_user.name, ''), r.pending, r.settle FROM records r LEFT JOIN users creditor_user ON creditor_user.id = r.creditor_user_id LEFT JOIN users debtor_user ON debtor_user.id = r.debtor_user_id WHERE r.owner_user_id = ? AND r.pending = 0 AND r.settle = 0 AND r.split_id IS NOT NULL AND ((r.debtor_user_id = ? AND r.creditor_user_id = ?) OR (r.debtor_user_id = ? AND r.creditor_user_id = ?)) ORDER BY r.date DESC, r.id DESC LIMIT ? OFFSET ?",
            (
                current_user.id.as_str(),
                current_user.id.as_str(),
                friend_id.as_str(),
                friend_id.as_str(),
                current_user.id.as_str(),
                limit,
                offset,
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query unsettled splits"))?;

    let mut splits = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| db_error())? {
        splits.push(split_list_item_from_row(row, &current_user.id)?);
    }

    Ok((
        StatusCode::OK,
        Json(SplitListResponse {
            splits,
            total_count,
            limit,
            offset,
        }),
    ))
}

fn split_list_item_from_row(
    row: libsql::Row,
    current_user_id: &str,
) -> Result<SplitListItem, (StatusCode, String)> {
    let record_id: String = row
        .get(0)
        .map_err(|_| db_error_with_context("invalid split list record id"))?;
    let split_id: Option<String> = row
        .get(1)
        .map_err(|_| db_error_with_context("invalid split list split id"))?;
    let description: String = row
        .get(2)
        .map_err(|_| db_error_with_context("invalid split list description"))?;
    let date: String = row
        .get(3)
        .map_err(|_| db_error_with_context("invalid split list date"))?;
    let amount: f64 = row
        .get(4)
        .map_err(|_| db_error_with_context("invalid split list amount"))?;
    let debtor_user_id: Option<String> = row
        .get(5)
        .map_err(|_| db_error_with_context("invalid split list debtor"))?;
    let creditor_user_id: Option<String> = row
        .get(6)
        .map_err(|_| db_error_with_context("invalid split list creditor"))?;
    let creditor_name: String = row
        .get(7)
        .map_err(|_| db_error_with_context("invalid split list creditor name"))?;
    let debtor_name: String = row
        .get(8)
        .map_err(|_| db_error_with_context("invalid split list debtor name"))?;
    let pending: bool = row
        .get(9)
        .map_err(|_| db_error_with_context("invalid split list pending flag"))?;
    let settle: bool = row
        .get(10)
        .map_err(|_| db_error_with_context("invalid split list settle flag"))?;

    let split_id =
        split_id.ok_or_else(|| db_error_with_context("split record missing split_id"))?;
    let debtor_user_id =
        debtor_user_id.ok_or_else(|| db_error_with_context("split record missing debtor user"))?;
    let creditor_user_id = creditor_user_id
        .ok_or_else(|| db_error_with_context("split record missing creditor user"))?;

    let requested_by_name = if creditor_name.trim().is_empty() {
        creditor_user_id.clone()
    } else {
        creditor_name
    };

    let (counterparty_user_id, counterparty_name, direction) =
        if debtor_user_id == current_user_id && creditor_user_id != current_user_id {
            (
                creditor_user_id.clone(),
                requested_by_name.clone(),
                "you_owe".to_string(),
            )
        } else if creditor_user_id == current_user_id && debtor_user_id != current_user_id {
            (
                debtor_user_id.clone(),
                if debtor_name.trim().is_empty() {
                    debtor_user_id.clone()
                } else {
                    debtor_name
                },
                "they_owe_you".to_string(),
            )
        } else {
            (
                creditor_user_id.clone(),
                requested_by_name.clone(),
                "you_owe".to_string(),
            )
        };

    Ok(SplitListItem {
        record_id,
        split_id,
        description,
        date,
        amount: amount.abs(),
        debtor_user_id,
        creditor_user_id: creditor_user_id.clone(),
        counterparty_user_id,
        counterparty_name,
        requested_by_user_id: creditor_user_id,
        requested_by_name,
        pending,
        settle,
        direction,
    })
}

fn validate_split_create_payload(
    payload: &CreateSplitPayload,
    initiator_user_id: &str,
) -> Result<(), (StatusCode, String)> {
    validate_string_length(
        &payload.idempotency_key,
        "Idempotency key",
        MAX_IDEMPOTENCY_KEY_LENGTH,
    )?;
    validate_string_length(&payload.description, "Description", 255)?;
    validate_string_length(&payload.category_id, "Category ID", 100)?;
    validate_date(&payload.date)?;
    validate_split_participants(&payload.splits, initiator_user_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    if !payload.total_amount.is_finite() || payload.total_amount <= 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Total amount must be a positive finite number".to_string(),
        ));
    }

    Ok(())
}

async fn validate_all_participants_are_friends(
    app_state: &AppState,
    current_user_id: &str,
    participants: &[SplitParticipant],
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.read().await;

    for participant in participants {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship WHERE from_user_id = ? AND to_user_id = ? AND pending = ?",
                (current_user_id, participant.user_id.as_str(), 0i64),
            )
            .await
            .map_err(|_| db_error_with_context("failed to validate friendship relation"))?;

        let count: i64 =
            if let Some(row) = rows.next().await.map_err(|_| {
                db_error_with_context("failed to fetch friendship validation result")
            })? {
                row.get(0)
                    .map_err(|_| db_error_with_context("invalid friendship validation result"))?
            } else {
                0
            };

        if count == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Participant {} is not an accepted friend",
                    participant.user_id
                ),
            ));
        }
    }

    Ok(())
}

async fn validate_friend_is_accepted(
    app_state: &AppState,
    current_user_id: &str,
    friend_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM friendship WHERE from_user_id = ? AND to_user_id = ? AND pending = ?",
            (current_user_id, friend_id, 0i64),
        )
        .await
        .map_err(|_| db_error_with_context("failed to validate friendship relation"))?;

    let count: i64 = if let Some(row) = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to fetch friendship validation result"))?
    {
        row.get(0)
            .map_err(|_| db_error_with_context("invalid friendship validation result"))?
    } else {
        0
    };

    if count == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Friend ID must be an accepted friend".to_string(),
        ));
    }

    Ok(())
}

async fn get_existing_idempotency_response(
    app_state: &AppState,
    user_id: &str,
    idempotency_key: &str,
) -> Result<Option<CachedIdempotency>, (StatusCode, String)> {
    let maybe_cached = {
        let conn = app_state.main_db.read().await;
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

async fn create_split_records(
    app_state: &AppState,
    initiator_user_id: &str,
    split_id: &str,
    payload: &CreateSplitPayload,
) -> Result<(String, Vec<String>), (StatusCode, String)> {
    let calculated = calculate_split_amounts(
        payload.total_amount,
        payload.splits.clone(),
        initiator_user_id,
    )
    .map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    validate_category_exists(&app_state.main_db, initiator_user_id, &payload.category_id).await?;

    let payer_record_id = Uuid::new_v4().to_string();
    let payer_amount = -(payload.total_amount.abs());

    // Pre-generate all pending record IDs before entering the transaction
    let pending_record_ids: Vec<String> = calculated
        .iter()
        .filter(|(uid, _)| uid != initiator_user_id)
        .map(|_| Uuid::new_v4().to_string())
        .collect();

    // Write all records atomically in one transaction on the shared DB
    {
        let pending_ids = pending_record_ids.clone();
        let description = payload.description.trim().to_string();
        let category_id = payload.category_id.trim().to_string();
        let date = payload.date.trim().to_string();
        let split_id_str = split_id.to_string();
        let initiator_id = initiator_user_id.to_string();
        let payer_id = payer_record_id.clone();
        let participants: Vec<(String, f64)> = calculated
            .iter()
            .filter(|(uid, _)| uid != initiator_user_id)
            .map(|(uid, amt)| (uid.clone(), *amt))
            .collect();

        with_transaction(&app_state.main_db, |conn| {
            let payer_id = payer_id.clone();
            let description = description.clone();
            let category_id = category_id.clone();
            let date = date.clone();
            let split_id_str = split_id_str.clone();
            let initiator_id = initiator_id.clone();
            let participants = participants.clone();
            let pending_ids = pending_ids.clone();

            Box::pin(async move {
                // Payer record
                conn.execute(
                    "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        payer_id.as_str(),
                        initiator_id.as_str(),
                        description.as_str(),
                        payer_amount,
                        category_id.as_str(),
                        date.as_str(),
                        false,
                        split_id_str.as_str(),
                        false,
                        initiator_id.as_str(),
                        initiator_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| SplitRecordError::Db)?;

                // Pending records for each participant
                for ((participant_user_id, amount), pending_record_id) in
                    participants.iter().zip(pending_ids.iter())
                {
                    let pending_amount = -(amount.abs());
                    conn.execute(
                        "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            pending_record_id.as_str(),
                            participant_user_id.as_str(),
                            description.as_str(),
                            pending_amount,
                            Option::<&str>::None,
                            date.as_str(),
                            true,
                            split_id_str.as_str(),
                            false,
                            participant_user_id.as_str(),
                            initiator_id.as_str(),
                        ),
                    )
                    .await
                    .map_err(|_| SplitRecordError::Db)?;
                }

                Ok::<(), SplitRecordError>(())
            })
        })
        .await
        .map_err(|_| db_error_with_context("failed to create split records"))?;
    }

    Ok((payer_record_id, pending_record_ids))
}

async fn reserve_idempotency_entry(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
    payload_hash: &str,
    created_at: &str,
    expires_at: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.write().await;
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

async fn commit_idempotency_entry(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
    response_status: i64,
    response_body: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.write().await;
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

async fn delete_idempotency_reservation(
    app_state: &AppState,
    idempotency_key: &str,
    user_id: &str,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.write().await;
    conn.execute(
        "DELETE FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ? AND response_body IS NULL",
        (idempotency_key, user_id, SPLIT_CREATE_ENDPOINT),
    )
    .await
    .map_err(|_| db_error_with_context("failed to delete idempotency reservation"))?;

    Ok(())
}

fn compute_payload_hash(payload: &CreateSplitPayload) -> Result<String, (StatusCode, String)> {
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

fn now_rfc3339() -> Result<String, (StatusCode, String)> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
