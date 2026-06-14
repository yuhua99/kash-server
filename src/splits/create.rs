use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::get_current_user;
use crate::errors::{db_error, db_error_with_context};
use crate::friends::ordered_user_pair;
use crate::models::{CreateSplitPayload, ParticipantBrief, SplitCreatedResponse, SplitParticipant};
use crate::money::{to_cents, to_decimal};
use crate::splits::{calculate_split_amounts, validate_split_participants};
use crate::validation::{
    validate_category_exists, validate_currency, validate_date, validate_string_length,
};
use crate::{AppState, TransactionError, with_transaction};

use super::idempotency::{
    IDEMPOTENCY_TTL_HOURS, MAX_IDEMPOTENCY_KEY_LENGTH, compute_payload_hash,
    delete_idempotency_reservation, get_existing_idempotency_response, now_rfc3339,
    reserve_idempotency_entry,
};

enum SplitRecordError {
    Transaction,
    Db,
    ReservationLost,
    NotFriend,
}

impl From<TransactionError> for SplitRecordError {
    fn from(_: TransactionError) -> Self {
        Self::Transaction
    }
}

pub async fn create_split(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<CreateSplitPayload>,
) -> Result<(StatusCode, Json<SplitCreatedResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_split_create_payload(&payload, &current_user.id)?;
    validate_all_participants_are_friends(&app_state, &current_user.id, &payload.splits).await?;

    let payload_hash = compute_payload_hash(&payload)?;
    if let Some(cached) =
        get_existing_idempotency_response(&app_state, &current_user.id, &payload.idempotency_key)
            .await?
    {
        return cached_split_response(cached, &payload_hash);
    }

    let split_id = Uuid::new_v4().to_string();
    let now = now_rfc3339()?;
    let expires_at = (time::OffsetDateTime::now_utc()
        + time::Duration::hours(IDEMPOTENCY_TTL_HOURS))
    .format(&time::format_description::well_known::Rfc3339)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let reservation_id = match reserve_idempotency_entry(
        &app_state,
        &payload.idempotency_key,
        &current_user.id,
        &payload_hash,
        &now,
        &expires_at,
    )
    .await
    {
        Ok(reservation_id) => reservation_id,
        Err(_) => {
            if let Some(cached) = get_existing_idempotency_response(
                &app_state,
                &current_user.id,
                &payload.idempotency_key,
            )
            .await?
            {
                return cached_split_response(cached, &payload_hash);
            }

            return Err(db_error_with_context("failed to reserve idempotency key"));
        }
    };

    let fanout_result = create_split_records(
        &app_state,
        &current_user.id,
        &split_id,
        &payload,
        &reservation_id,
        &now,
    )
    .await;

    let response = match fanout_result {
        Ok(response) => response,
        Err(e) => {
            if e != (
                StatusCode::CONFLICT,
                "A request with this idempotency key is already in progress".to_string(),
            ) {
                let _ = delete_idempotency_reservation(&app_state, &reservation_id).await;
            }
            return Err(e);
        }
    };

    Ok((StatusCode::CREATED, Json(response)))
}

fn cached_split_response(
    cached: super::idempotency::CachedIdempotency,
    payload_hash: &str,
) -> Result<(StatusCode, Json<SplitCreatedResponse>), (StatusCode, String)> {
    if cached.payload_hash != payload_hash {
        return Err((
            StatusCode::CONFLICT,
            "Idempotency key already used with different payload".to_string(),
        ));
    }

    let response =
        serde_json::from_str::<SplitCreatedResponse>(&cached.response_body).map_err(|_| {
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

    Ok((status, Json(response)))
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

    if payload.total_amount > crate::constants::MAX_RECORD_AMOUNT {
        return Err((
            StatusCode::BAD_REQUEST,
            "Total amount exceeds maximum allowed value".to_string(),
        ));
    }

    Ok(())
}

async fn validate_all_participants_are_friends(
    app_state: &AppState,
    current_user_id: &str,
    participants: &[SplitParticipant],
) -> Result<(), (StatusCode, String)> {
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    for participant in participants {
        let (user_low_id, user_high_id) = ordered_user_pair(current_user_id, &participant.user_id);
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM friendship WHERE user_low_id = ? AND user_high_id = ? AND pending = ?",
                (user_low_id, user_high_id, 0i64),
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
    reservation_id: &str,
    created_at: &str,
) -> Result<SplitCreatedResponse, (StatusCode, String)> {
    let calculated = calculate_split_amounts(
        payload.total_amount,
        payload.splits.clone(),
        initiator_user_id,
    )
    .map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;

    validate_category_exists(&app_state.main_db, initiator_user_id, &payload.category_id).await?;

    let creditor_record_id = Uuid::new_v4().to_string();
    let initiator_share = calculated
        .iter()
        .find(|(user_id, _)| user_id == initiator_user_id)
        .map(|(_, amount)| *amount)
        .ok_or_else(|| db_error_with_context("split calculation missing initiator share"))?;
    let creditor_amount = -initiator_share.abs();

    let participants: Vec<(String, String, i64)> = calculated
        .iter()
        .filter(|(uid, _)| uid != initiator_user_id)
        .map(|(uid, amt)| (Uuid::new_v4().to_string(), uid.clone(), amt.abs()))
        .collect();

    let response = SplitCreatedResponse {
        split_id: split_id.to_string(),
        creditor_record_id: creditor_record_id.clone(),
        participants: participants
            .iter()
            .map(|(id, debtor_user_id, amount)| ParticipantBrief {
                id: id.clone(),
                debtor_user_id: debtor_user_id.clone(),
                amount: to_decimal(*amount),
            })
            .collect(),
    };
    let response_body = serde_json::to_string(&response).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize response: {}", e),
        )
    })?;

    {
        let description = payload.description.trim().to_string();
        let category_id = payload.category_id.trim().to_string();
        let currency = validate_currency(&payload.currency)?;
        let date = payload.date.trim().to_string();
        let split_id = split_id.to_string();
        let initiator_id = initiator_user_id.to_string();
        let total_amount = to_cents(payload.total_amount);
        let created_at = created_at.to_string();
        let reservation_id = reservation_id.to_string();

        with_transaction(&app_state.main_db, |conn| {
            let creditor_record_id = creditor_record_id.clone();
            let description = description.clone();
            let category_id = category_id.clone();
            let currency = currency.clone();
            let date = date.clone();
            let split_id = split_id.clone();
            let initiator_id = initiator_id.clone();
            let participants = participants.clone();
            let created_at = created_at.clone();
            let reservation_id = reservation_id.clone();
            let response_body = response_body.clone();

            Box::pin(async move {
                conn.execute(
                    "INSERT INTO splits (id, creditor_user_id, description, currency, date, total_amount, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        split_id.as_str(),
                        initiator_id.as_str(),
                        description.as_str(),
                        currency.as_str(),
                        date.as_str(),
                        total_amount,
                        created_at.as_str(),
                    ),
                )
                .await
                .map_err(|_| SplitRecordError::Db)?;

                conn.execute(
                    "INSERT INTO records (id, owner_user_id, name, amount, currency, category_id, date) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        creditor_record_id.as_str(),
                        initiator_id.as_str(),
                        description.as_str(),
                        creditor_amount,
                        currency.as_str(),
                        category_id.as_str(),
                        date.as_str(),
                    ),
                )
                .await
                .map_err(|_| SplitRecordError::Db)?;

                for (_, debtor_user_id, _) in participants.iter() {
                    let (user_low_id, user_high_id) =
                        ordered_user_pair(initiator_id.as_str(), debtor_user_id.as_str());
                    let mut rows = conn
                        .query(
                            "SELECT COUNT(*) FROM friendship WHERE user_low_id = ? AND user_high_id = ? AND pending = 0",
                            (user_low_id, user_high_id),
                        )
                        .await
                        .map_err(|_| SplitRecordError::Db)?;
                    let count: i64 = if let Some(row) =
                        rows.next().await.map_err(|_| SplitRecordError::Db)?
                    {
                        row.get(0).map_err(|_| SplitRecordError::Db)?
                    } else {
                        0
                    };
                    drop(rows);

                    if count == 0 {
                        return Err(SplitRecordError::NotFriend);
                    }
                }

                for (participant_id, debtor_user_id, amount) in participants.iter() {
                    conn.execute(
                        "INSERT INTO split_participants (id, split_id, debtor_user_id, amount, settled, finalized_record_id) VALUES (?, ?, ?, ?, ?, ?)",
                        (
                            participant_id.as_str(),
                            split_id.as_str(),
                            debtor_user_id.as_str(),
                            *amount,
                            false,
                            Option::<&str>::None,
                        ),
                    )
                    .await
                    .map_err(|_| SplitRecordError::Db)?;
                }

                let updated = conn
                    .execute(
                        "UPDATE idempotency_keys SET response_status = ?, response_body = ? WHERE id = ? AND response_body IS NULL",
                        (
                            i64::from(StatusCode::CREATED.as_u16()),
                            response_body.as_str(),
                            reservation_id.as_str(),
                        ),
                    )
                    .await
                    .map_err(|_| SplitRecordError::Db)?;

                if updated != 1 {
                    return Err(SplitRecordError::ReservationLost);
                }

                Ok::<(), SplitRecordError>(())
            })
        })
        .await
        .map_err(|e| match e {
            SplitRecordError::ReservationLost => (
                StatusCode::CONFLICT,
                "A request with this idempotency key is already in progress".to_string(),
            ),
            SplitRecordError::NotFriend => (
                StatusCode::BAD_REQUEST,
                "Participant is no longer an accepted friend".to_string(),
            ),
            SplitRecordError::Transaction | SplitRecordError::Db => {
                db_error_with_context("failed to create split records")
            }
        })?;
    }

    Ok(response)
}
