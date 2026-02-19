use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::auth::get_current_user;
use crate::constants::{
    FRIEND_STATUS_ACCEPTED, SPLIT_STATUS_COMPLETED, SPLIT_STATUS_INITIATED,
    SPLIT_STATUS_PARTIAL_FAILURE,
};
use crate::db_pool::{TransactionError, with_transaction};
use crate::models::{CreateSplitPayload, SplitParticipant};
use crate::utils::{
    calculate_split_amounts, db_error_with_context, validate_category_exists, validate_date,
    validate_split_participants, validate_string_length,
};

const SPLIT_CREATE_ENDPOINT: &str = "/splits/create";
const IDEMPOTENCY_TTL_HOURS: i64 = 24;
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 255;

#[derive(Debug)]
enum SplitCreateError {
    Transaction(TransactionError),
    Db(&'static str),
    BadRequest(String),
}

impl From<TransactionError> for SplitCreateError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<SplitCreateError> for (StatusCode, String) {
    fn from(value: SplitCreateError) -> Self {
        match value {
            SplitCreateError::Transaction(TransactionError::Begin) => {
                db_error_with_context("failed to begin transaction")
            }
            SplitCreateError::Transaction(TransactionError::Commit) => {
                db_error_with_context("failed to commit transaction")
            }
            SplitCreateError::Db(ctx) => db_error_with_context(ctx),
            SplitCreateError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateSplitResponse {
    pub split_id: String,
    pub payer_record_id: String,
    pub pending_record_ids: Vec<String>,
}

struct FanoutSuccess {
    payer_record_id: String,
    pending_record_ids: Vec<String>,
    succeeded_participant_ids: Vec<String>,
}

struct FanoutFailure {
    error: SplitCreateError,
    succeeded_participant_ids: Vec<String>,
    failed_participant_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct RetrySplitResponse {
    pub split_id: String,
    pub status: String,
    pub pending_record_ids: Vec<String>,
    pub missing_participant_ids: Vec<String>,
}

struct SplitCoordinationState {
    id: String,
    initiator_user_id: String,
    payload_json: String,
    fanout_attempts: i64,
}

struct CachedIdempotency {
    response_status: i64,
    response_body: String,
    payload_hash: String,
}

struct IdempotencyInsert<'a> {
    idempotency_key: &'a str,
    user_id: &'a str,
    payload_hash: &'a str,
    response_status: i64,
    response_body: &'a str,
    created_at: &'a str,
    expires_at: &'a str,
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

    insert_split_coordination(&app_state, &split_id, &current_user.id, &payload, &now).await?;

    let fanout_result =
        create_split_records_fanout(&app_state, &current_user.id, &split_id, &payload).await;

    let fanout = match fanout_result {
        Ok(success) => success,
        Err(failure) => {
            update_split_coordination_progress(
                &app_state,
                &split_id,
                SPLIT_STATUS_PARTIAL_FAILURE,
                &failure.succeeded_participant_ids,
                &failure.failed_participant_ids,
                1,
                &now,
            )
            .await?;

            let mut base_error: (StatusCode, String) = failure.error.into();
            base_error.1 = format!("{}: {}", base_error.1, split_id);
            return Err(base_error);
        }
    };

    update_split_coordination_progress(
        &app_state,
        &split_id,
        SPLIT_STATUS_INITIATED,
        &fanout.succeeded_participant_ids,
        &Vec::new(),
        1,
        &now,
    )
    .await?;

    let response = CreateSplitResponse {
        split_id,
        payer_record_id: fanout.payer_record_id,
        pending_record_ids: fanout.pending_record_ids,
    };

    let response_body = serde_json::to_string(&response).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize response: {}", e),
        )
    })?;

    store_idempotency_entry(
        &app_state,
        &IdempotencyInsert {
            idempotency_key: &payload.idempotency_key,
            user_id: &current_user.id,
            payload_hash: &payload_hash,
            response_status: i64::from(StatusCode::CREATED.as_u16()),
            response_body: &response_body,
            created_at: &now,
            expires_at: &expires_at,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn retry_split_fanout(
    State(app_state): State<AppState>,
    session: Session,
    Path(split_id): Path<String>,
) -> Result<(StatusCode, Json<RetrySplitResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let split = get_split_coordination_state(&app_state, &split_id).await?;

    if split.initiator_user_id != current_user.id {
        return Err((StatusCode::NOT_FOUND, "Split not found".to_string()));
    }

    let payload: CreateSplitPayload = serde_json::from_str(&split.payload_json)
        .map_err(|_| db_error_with_context("failed to parse split payload_json"))?;
    let now = now_rfc3339()?;

    let fanout_result =
        create_split_records_fanout(&app_state, &split.initiator_user_id, &split.id, &payload)
            .await;

    match fanout_result {
        Ok(success) => {
            update_split_coordination_progress(
                &app_state,
                &split.id,
                SPLIT_STATUS_COMPLETED,
                &success.succeeded_participant_ids,
                &Vec::new(),
                split.fanout_attempts + 1,
                &now,
            )
            .await?;

            Ok((
                StatusCode::OK,
                Json(RetrySplitResponse {
                    split_id: split.id,
                    status: SPLIT_STATUS_COMPLETED.to_string(),
                    pending_record_ids: success.pending_record_ids,
                    missing_participant_ids: Vec::new(),
                }),
            ))
        }
        Err(failure) => {
            update_split_coordination_progress(
                &app_state,
                &split.id,
                SPLIT_STATUS_PARTIAL_FAILURE,
                &failure.succeeded_participant_ids,
                &failure.failed_participant_ids,
                split.fanout_attempts + 1,
                &now,
            )
            .await?;

            Ok((
                StatusCode::OK,
                Json(RetrySplitResponse {
                    split_id: split.id,
                    status: SPLIT_STATUS_PARTIAL_FAILURE.to_string(),
                    pending_record_ids: Vec::new(),
                    missing_participant_ids: failure.failed_participant_ids,
                }),
            ))
        }
    }
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
                "SELECT COUNT(*) FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ? AND status = ?",
                (
                    current_user_id,
                    participant.user_id.as_str(),
                    FRIEND_STATUS_ACCEPTED,
                ),
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

async fn get_existing_idempotency_response(
    app_state: &AppState,
    user_id: &str,
    idempotency_key: &str,
) -> Result<Option<CachedIdempotency>, (StatusCode, String)> {
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
        let response_body: String = row
            .get(1)
            .map_err(|_| db_error_with_context("invalid idempotency response body"))?;
        let payload_hash: String = row
            .get(2)
            .map_err(|_| db_error_with_context("invalid idempotency payload hash"))?;

        Ok(Some(CachedIdempotency {
            response_status,
            response_body,
            payload_hash,
        }))
    } else {
        Ok(None)
    }
}

async fn insert_split_coordination(
    app_state: &AppState,
    split_id: &str,
    initiator_user_id: &str,
    payload: &CreateSplitPayload,
    now: &str,
) -> Result<(), (StatusCode, String)> {
    let payload_json = serde_json::to_string(payload)
        .map_err(|_| db_error_with_context("failed to serialize split payload"))?;

    let conn = app_state.main_db.write().await;
    conn.execute(
        "INSERT INTO split_coordination (id, initiator_user_id, idempotency_key, status, total_amount, participant_count, payload_json, succeeded_participant_ids, failed_participant_ids, fanout_attempts, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (
            split_id,
            initiator_user_id,
            payload.idempotency_key.as_str(),
            SPLIT_STATUS_INITIATED,
            payload.total_amount,
            payload.splits.len() as i64 + 1,
            payload_json.as_str(),
            "[]",
            "[]",
            0,
            now,
            now,
        ),
    )
    .await
    .map_err(|_| db_error_with_context("failed to create split coordination"))?;

    Ok(())
}

async fn create_split_records_fanout(
    app_state: &AppState,
    initiator_user_id: &str,
    split_id: &str,
    payload: &CreateSplitPayload,
) -> Result<FanoutSuccess, FanoutFailure> {
    let calculated = calculate_split_amounts(
        payload.total_amount,
        payload.splits.clone(),
        initiator_user_id,
    )
    .map_err(|msg| FanoutFailure {
        error: SplitCreateError::BadRequest(msg),
        succeeded_participant_ids: Vec::new(),
        failed_participant_ids: payload
            .splits
            .iter()
            .map(|p| p.user_id.clone())
            .chain(std::iter::once(initiator_user_id.to_string()))
            .collect(),
    })?;

    let mut succeeded_participant_ids: Vec<String> = Vec::new();

    let user_db = app_state
        .db_pool
        .get_user_db(initiator_user_id)
        .await
        .map_err(|_| FanoutFailure {
            error: SplitCreateError::Db("failed to access payer database"),
            succeeded_participant_ids: succeeded_participant_ids.clone(),
            failed_participant_ids: all_participant_ids(&calculated, initiator_user_id),
        })?;

    let payer_record_id = match find_existing_payer_record_id(&user_db, split_id, initiator_user_id)
        .await
    {
        Ok(Some(existing)) => existing,
        Ok(None) => {
            validate_category_exists(&user_db, &payload.category_id)
                .await
                .map_err(|_| FanoutFailure {
                    error: SplitCreateError::BadRequest("Category does not exist".to_string()),
                    succeeded_participant_ids: succeeded_participant_ids.clone(),
                    failed_participant_ids: all_participant_ids(&calculated, initiator_user_id),
                })?;

            let payer_record_id = Uuid::new_v4().to_string();
            let payer_amount = -(payload.total_amount.abs());

            with_transaction(&user_db, |conn| {
                let payer_record_id = payer_record_id.clone();
                let description = payload.description.trim().to_string();
                let category_id = payload.category_id.trim().to_string();
                let date = payload.date.trim().to_string();
                let split_id = split_id.to_string();
                let initiator_user_id = initiator_user_id.to_string();

                Box::pin(async move {
                    conn.execute(
                        "INSERT INTO records (id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            payer_record_id.as_str(),
                            description.as_str(),
                            payer_amount,
                            category_id.as_str(),
                            date.as_str(),
                            false,
                            split_id.as_str(),
                            false,
                            initiator_user_id.as_str(),
                            initiator_user_id.as_str(),
                        ),
                    )
                    .await
                    .map_err(|_| SplitCreateError::Db("failed to create payer record"))?;

                    Ok(())
                })
            })
            .await
            .map_err(|err| FanoutFailure {
                error: err,
                succeeded_participant_ids: succeeded_participant_ids.clone(),
                failed_participant_ids: all_participant_ids(&calculated, initiator_user_id),
            })?;

            payer_record_id
        }
        Err(_) => {
            return Err(FanoutFailure {
                error: SplitCreateError::Db("failed to check payer record status"),
                succeeded_participant_ids: succeeded_participant_ids.clone(),
                failed_participant_ids: all_participant_ids(&calculated, initiator_user_id),
            });
        }
    };

    succeeded_participant_ids.push(initiator_user_id.to_string());

    let mut pending_record_ids = Vec::new();

    for (index, (participant_user_id, amount)) in calculated.iter().enumerate() {
        if participant_user_id == initiator_user_id {
            continue;
        }

        if let Ok(Some(existing_pending_id)) = find_existing_pending_record_id(
            app_state,
            participant_user_id,
            split_id,
            initiator_user_id,
        )
        .await
        {
            succeeded_participant_ids.push(participant_user_id.clone());
            pending_record_ids.push(existing_pending_id);
            continue;
        }

        if should_fail_participant_once(app_state, participant_user_id)
            .await
            .unwrap_or(false)
        {
            return Err(FanoutFailure {
                error: SplitCreateError::Db("failed to create participant pending record"),
                succeeded_participant_ids,
                failed_participant_ids: remaining_participant_ids(&calculated, index),
            });
        }

        let participant_db = app_state
            .db_pool
            .get_user_db(participant_user_id)
            .await
            .map_err(|_| FanoutFailure {
                error: SplitCreateError::Db("failed to access participant database"),
                succeeded_participant_ids: succeeded_participant_ids.clone(),
                failed_participant_ids: remaining_participant_ids(&calculated, index),
            })?;

        let pending_record_id = Uuid::new_v4().to_string();
        let pending_amount = -(amount.abs());

        with_transaction(&participant_db, |conn| {
            let pending_record_id = pending_record_id.clone();
            let description = payload.description.trim().to_string();
            let date = payload.date.trim().to_string();
            let split_id = split_id.to_string();
            let participant_user_id = participant_user_id.to_string();
            let initiator_user_id = initiator_user_id.to_string();

            Box::pin(async move {
                conn.execute(
                    "INSERT INTO records (id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        pending_record_id.as_str(),
                        description.as_str(),
                        pending_amount,
                        Option::<&str>::None,
                        date.as_str(),
                        true,
                        split_id.as_str(),
                        false,
                        participant_user_id.as_str(),
                        initiator_user_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| SplitCreateError::Db("failed to create participant pending record"))?;

                Ok(())
            })
        })
        .await
        .map_err(|err| FanoutFailure {
            error: err,
            succeeded_participant_ids: succeeded_participant_ids.clone(),
            failed_participant_ids: remaining_participant_ids(&calculated, index),
        })?;

        succeeded_participant_ids.push(participant_user_id.to_string());
        pending_record_ids.push(pending_record_id);
    }

    Ok(FanoutSuccess {
        payer_record_id,
        pending_record_ids,
        succeeded_participant_ids,
    })
}

fn all_participant_ids(calculated: &[(String, f64)], initiator_user_id: &str) -> Vec<String> {
    let mut result = Vec::with_capacity(calculated.len());
    result.push(initiator_user_id.to_string());
    for (user_id, _) in calculated {
        if user_id != initiator_user_id {
            result.push(user_id.clone());
        }
    }
    result
}

fn remaining_participant_ids(calculated: &[(String, f64)], failing_index: usize) -> Vec<String> {
    let mut result = Vec::new();
    for (idx, (user_id, _)) in calculated.iter().enumerate() {
        if idx >= failing_index {
            result.push(user_id.clone());
        }
    }
    result
}

async fn find_existing_payer_record_id(
    user_db: &crate::db_pool::DbConnection,
    split_id: &str,
    initiator_user_id: &str,
) -> Result<Option<String>, (StatusCode, String)> {
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id FROM records WHERE split_id = ? AND pending = ? AND debtor_user_id = ? AND creditor_user_id = ? LIMIT 1",
            (split_id, false, initiator_user_id, initiator_user_id),
        )
        .await
        .map_err(|_| db_error_with_context("failed to check existing payer record"))?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to read payer record row"))?
    {
        let id: String = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid payer record id"))?;
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

async fn find_existing_pending_record_id(
    app_state: &AppState,
    participant_user_id: &str,
    split_id: &str,
    initiator_user_id: &str,
) -> Result<Option<String>, (StatusCode, String)> {
    let participant_db = app_state
        .db_pool
        .get_user_db(participant_user_id)
        .await
        .map_err(|_| db_error_with_context("failed to access participant database"))?;

    let conn = participant_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id FROM records WHERE split_id = ? AND pending = ? AND debtor_user_id = ? AND creditor_user_id = ? LIMIT 1",
            (split_id, true, participant_user_id, initiator_user_id),
        )
        .await
        .map_err(|_| db_error_with_context("failed to check existing pending record"))?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to read pending record row"))?
    {
        let id: String = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid pending record id"))?;
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

async fn should_fail_participant_once(
    app_state: &AppState,
    participant_user_id: &str,
) -> Result<bool, (StatusCode, String)> {
    let conn = app_state.main_db.write().await;
    let mut rows = match conn
        .query(
            "SELECT fail_once FROM split_failure_injections WHERE user_id = ?",
            [participant_user_id],
        )
        .await
    {
        Ok(rows) => rows,
        Err(_) => return Ok(false),
    };

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to read split failure injection row"))?
    {
        let fail_once: i64 = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid split failure injection value"))?;
        if fail_once == 1 {
            let _ = conn
                .execute(
                    "DELETE FROM split_failure_injections WHERE user_id = ?",
                    [participant_user_id],
                )
                .await;
            return Ok(true);
        }
    }

    Ok(false)
}

async fn get_split_coordination_state(
    app_state: &AppState,
    split_id: &str,
) -> Result<SplitCoordinationState, (StatusCode, String)> {
    let conn = app_state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, initiator_user_id, payload_json, fanout_attempts FROM split_coordination WHERE id = ?",
            [split_id],
        )
        .await
        .map_err(|_| db_error_with_context("failed to query split coordination state"))?;

    let row = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to read split coordination state row"))?
        .ok_or((StatusCode::NOT_FOUND, "Split not found".to_string()))?;

    Ok(SplitCoordinationState {
        id: row
            .get(0)
            .map_err(|_| db_error_with_context("invalid split id"))?,
        initiator_user_id: row
            .get(1)
            .map_err(|_| db_error_with_context("invalid split initiator_user_id"))?,
        payload_json: row
            .get(2)
            .map_err(|_| db_error_with_context("invalid split payload_json"))?,
        fanout_attempts: row
            .get(3)
            .map_err(|_| db_error_with_context("invalid split fanout_attempts"))?,
    })
}

async fn update_split_coordination_progress(
    app_state: &AppState,
    split_id: &str,
    status: &str,
    succeeded_participant_ids: &[String],
    failed_participant_ids: &[String],
    fanout_attempts: i64,
    now: &str,
) -> Result<(), (StatusCode, String)> {
    let succeeded_json = serde_json::to_string(succeeded_participant_ids)
        .map_err(|_| db_error_with_context("failed to serialize succeeded_participant_ids"))?;
    let failed_json = serde_json::to_string(failed_participant_ids)
        .map_err(|_| db_error_with_context("failed to serialize failed_participant_ids"))?;

    let conn = app_state.main_db.write().await;
    conn.execute(
        "UPDATE split_coordination SET status = ?, succeeded_participant_ids = ?, failed_participant_ids = ?, fanout_attempts = ?, updated_at = ? WHERE id = ?",
        (status, succeeded_json.as_str(), failed_json.as_str(), fanout_attempts, now, split_id),
    )
    .await
    .map_err(|_| db_error_with_context("failed to update split coordination progress"))?;

    Ok(())
}

async fn store_idempotency_entry(
    app_state: &AppState,
    entry: &IdempotencyInsert<'_>,
) -> Result<(), (StatusCode, String)> {
    let conn = app_state.main_db.write().await;
    conn.execute(
        "INSERT INTO idempotency_keys (key, user_id, endpoint, payload_hash, response_status, response_body, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        (
            entry.idempotency_key,
            entry.user_id,
            SPLIT_CREATE_ENDPOINT,
            entry.payload_hash,
            entry.response_status,
            entry.response_body,
            entry.created_at,
            entry.expires_at,
        ),
    )
    .await
    .map_err(|_| db_error_with_context("failed to store idempotency key"))?;

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
