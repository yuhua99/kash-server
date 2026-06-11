use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{CreateSplitPayload, SplitParticipant};
use crate::splits::{calculate_split_amounts, validate_split_participants};
use crate::validation::{
    validate_category_exists, validate_currency, validate_date, validate_string_length,
};
use crate::{AppState, TransactionError, with_transaction};

use super::idempotency::{
    IDEMPOTENCY_TTL_HOURS, MAX_IDEMPOTENCY_KEY_LENGTH, commit_idempotency_entry,
    compute_payload_hash, delete_idempotency_reservation, get_existing_idempotency_response,
    now_rfc3339, reserve_idempotency_entry,
};

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
    validate_currency(&payload.currency)?;
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
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;

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
    let initiator_share = calculated
        .iter()
        .find(|(user_id, _)| user_id == initiator_user_id)
        .map(|(_, amount)| *amount)
        .ok_or_else(|| db_error_with_context("split calculation missing initiator share"))?;
    let payer_amount = if initiator_share == 0.0 {
        0.0
    } else {
        -initiator_share.abs()
    };

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
        let currency = validate_currency(&payload.currency)?;
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
            let currency = currency.clone();
            let date = date.clone();
            let split_id_str = split_id_str.clone();
            let initiator_id = initiator_id.clone();
            let participants = participants.clone();
            let pending_ids = pending_ids.clone();

            Box::pin(async move {
                // Payer record
                conn.execute(
                    "INSERT INTO records (id, owner_user_id, name, amount, currency, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        payer_id.as_str(),
                        initiator_id.as_str(),
                        description.as_str(),
                        payer_amount,
                        currency.as_str(),
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
                        "INSERT INTO records (id, owner_user_id, name, amount, currency, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            pending_record_id.as_str(),
                            participant_user_id.as_str(),
                            description.as_str(),
                            pending_amount,
                            currency.as_str(),
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
