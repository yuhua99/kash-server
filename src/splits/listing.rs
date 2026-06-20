use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{
    PendingShare, PendingShareListResponse, PendingSharesQuery, UnsettledShare,
    UnsettledShareListResponse, UnsettledSharesQuery,
};
use crate::money::to_decimal;
use crate::validation::{validate_offset, validate_records_limit, validate_string_length};

#[utoipa::path(
    get,
    path = "/splits/pending",
    tag = "splits",
    params(crate::models::PendingSharesQuery),
    responses(
        (status = 200, description = "Pending shares", body = crate::models::PendingShareListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn list_pending_shares(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<PendingSharesQuery>,
) -> Result<(StatusCode, Json<PendingShareListResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*) FROM split_participants WHERE debtor_user_id = ? AND finalized_record_id IS NULL",
            [current_user.id.as_str()],
        )
        .await
        .map_err(|_| db_error_with_context("failed to count pending shares"))?;
    let total_count_i64: i64 = if let Some(row) = count_rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to count pending shares"))?
    {
        row.get(0)
            .map_err(|_| db_error_with_context("failed to count pending shares"))?
    } else {
        0
    };
    let total_count = u32::try_from(total_count_i64)
        .map_err(|_| db_error_with_context("pending share count overflow"))?;
    drop(count_rows);

    let mut rows = conn
        .query(
            "SELECT sp.id, sp.split_id, s.description, s.date, sp.amount, s.currency, s.creditor_user_id, COALESCE(cu.name, ''), sp.settled
             FROM split_participants sp
             JOIN splits s ON s.id = sp.split_id
             LEFT JOIN users cu ON cu.id = s.creditor_user_id
             WHERE sp.debtor_user_id = ? AND sp.finalized_record_id IS NULL
             ORDER BY s.date DESC, sp.id DESC LIMIT ? OFFSET ?",
            (current_user.id.as_str(), limit, offset),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query pending shares"))?;

    let mut shares = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to read pending shares"))?
    {
        let participant_id: String = row
            .get(0)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let split_id: String = row
            .get(1)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let description: String = row
            .get(2)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let date: String = row
            .get(3)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let amount_cents: i64 = row
            .get(4)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let currency: String = row
            .get(5)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let creditor_user_id: String = row
            .get(6)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let creditor_name: String = row
            .get(7)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let settled: bool = row
            .get(8)
            .map_err(|_| db_error_with_context("failed to read pending share"))?;
        let creditor_name = if creditor_name.is_empty() {
            creditor_user_id.clone()
        } else {
            creditor_name
        };

        shares.push(PendingShare {
            participant_id,
            split_id,
            description,
            date,
            amount: to_decimal(amount_cents),
            currency,
            creditor_user_id,
            creditor_name,
            settled,
        });
    }

    Ok((
        StatusCode::OK,
        Json(PendingShareListResponse {
            shares,
            total_count,
            limit,
            offset,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/splits/unsettled",
    tag = "splits",
    params(crate::models::UnsettledSharesQuery),
    responses(
        (status = 200, description = "Unsettled shares", body = crate::models::UnsettledShareListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn list_unsettled_shares(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<UnsettledSharesQuery>,
) -> Result<(StatusCode, Json<UnsettledShareListResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_string_length(
        &query.friend_id,
        "Friend ID",
        crate::constants::MAX_RECORD_NAME_LENGTH,
    )?;
    let friend_id = query.friend_id.trim();
    if friend_id == current_user.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Friend ID cannot be your own user ID".to_string(),
        ));
    }
    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .map_err(|_| db_error())?;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*)
             FROM split_participants sp
             JOIN splits s ON s.id = sp.split_id
             WHERE sp.settled = 0 AND ((sp.debtor_user_id = ? AND s.creditor_user_id = ?) OR (sp.debtor_user_id = ? AND s.creditor_user_id = ?))",
            (
                current_user.id.as_str(),
                friend_id,
                friend_id,
                current_user.id.as_str(),
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to count unsettled shares"))?;
    let total_count_i64: i64 = if let Some(row) = count_rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to count unsettled shares"))?
    {
        row.get(0)
            .map_err(|_| db_error_with_context("failed to count unsettled shares"))?
    } else {
        0
    };
    let total_count = u32::try_from(total_count_i64)
        .map_err(|_| db_error_with_context("unsettled share count overflow"))?;
    drop(count_rows);

    let mut rows = conn
        .query(
            "SELECT sp.id, sp.split_id, s.description, s.date, sp.amount, s.currency, s.creditor_user_id, sp.debtor_user_id, COALESCE(cu.name,''), COALESCE(du.name,''), (sp.finalized_record_id IS NOT NULL), sp.settled
             FROM split_participants sp
             JOIN splits s ON s.id = sp.split_id
             LEFT JOIN users cu ON cu.id = s.creditor_user_id
             LEFT JOIN users du ON du.id = sp.debtor_user_id
             WHERE sp.settled = 0 AND ((sp.debtor_user_id = ? AND s.creditor_user_id = ?) OR (sp.debtor_user_id = ? AND s.creditor_user_id = ?))
             ORDER BY s.date DESC, sp.id DESC LIMIT ? OFFSET ?",
            (
                current_user.id.as_str(),
                friend_id,
                friend_id,
                current_user.id.as_str(),
                limit,
                offset,
            ),
        )
        .await
        .map_err(|_| db_error_with_context("failed to query unsettled shares"))?;

    let mut shares = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| db_error_with_context("failed to read unsettled shares"))?
    {
        let participant_id: String = row
            .get(0)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let split_id: String = row
            .get(1)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let description: String = row
            .get(2)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let date: String = row
            .get(3)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let amount_cents: i64 = row
            .get(4)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let currency: String = row
            .get(5)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let creditor_user_id: String = row
            .get(6)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let debtor_user_id: String = row
            .get(7)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let creditor_name: String = row
            .get(8)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let debtor_name: String = row
            .get(9)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let finalized: bool = row
            .get(10)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;
        let settled: bool = row
            .get(11)
            .map_err(|_| db_error_with_context("failed to read unsettled share"))?;

        let (direction, counterparty_user_id, counterparty_name) =
            if debtor_user_id == current_user.id {
                let counterparty_name = if creditor_name.is_empty() {
                    creditor_user_id.clone()
                } else {
                    creditor_name
                };
                ("you_owe".to_string(), creditor_user_id, counterparty_name)
            } else {
                let counterparty_name = if debtor_name.is_empty() {
                    debtor_user_id.clone()
                } else {
                    debtor_name
                };
                (
                    "they_owe_you".to_string(),
                    debtor_user_id,
                    counterparty_name,
                )
            };

        shares.push(UnsettledShare {
            participant_id,
            split_id,
            description,
            date,
            amount: to_decimal(amount_cents),
            currency,
            direction,
            counterparty_user_id,
            counterparty_name,
            finalized,
            settled,
        });
    }

    Ok((
        StatusCode::OK,
        Json(UnsettledShareListResponse {
            shares,
            total_count,
            limit,
            offset,
        }),
    ))
}
