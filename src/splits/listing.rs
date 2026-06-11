use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde_json::json;
use tower_sessions::Session;

use crate::auth::get_current_user;
use crate::constants::MAX_RECORD_NAME_LENGTH;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{PendingSplitsQuery, SplitListItem, SplitListResponse, UnsettledSplitsQuery};
use crate::money::to_decimal;
use crate::validation::{validate_offset, validate_records_limit, validate_string_length};
use crate::{AppState, TransactionError, with_transaction};

pub async fn list_pending_splits(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<PendingSplitsQuery>,
) -> Result<(StatusCode, Json<SplitListResponse>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;

    let conn = app_state.main_db.connect().map_err(|_| db_error())?;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ? AND split_id IS NOT NULL AND category_id IS NULL",
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
            "SELECT r.id, r.split_id, r.name, r.date, r.amount, r.currency, r.owner_user_id, r.creditor_user_id, COALESCE(creditor_user.name, ''), COALESCE(debtor_user.name, ''), (r.split_id IS NOT NULL AND r.category_id IS NULL), r.settle FROM records r LEFT JOIN users creditor_user ON creditor_user.id = r.creditor_user_id LEFT JOIN users debtor_user ON debtor_user.id = r.owner_user_id WHERE r.owner_user_id = ? AND r.split_id IS NOT NULL AND r.category_id IS NULL ORDER BY r.date DESC, r.id DESC LIMIT ? OFFSET ?",
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

    let limit = validate_records_limit(query.limit)?;
    let offset = validate_offset(query.offset)?;

    let conn = app_state.main_db.connect().map_err(|_| db_error())?;

    let mut count_rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id IN (?, ?) AND NOT (split_id IS NOT NULL AND category_id IS NULL) AND settle = 0 AND split_id IS NOT NULL AND ((owner_user_id = ? AND creditor_user_id = ?) OR (owner_user_id = ? AND creditor_user_id = ?))",
            (
                current_user.id.as_str(),
                friend_id.as_str(),
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
            "SELECT r.id, r.split_id, r.name, r.date, r.amount, r.currency, r.owner_user_id, r.creditor_user_id, COALESCE(creditor_user.name, ''), COALESCE(debtor_user.name, ''), (r.split_id IS NOT NULL AND r.category_id IS NULL), r.settle FROM records r LEFT JOIN users creditor_user ON creditor_user.id = r.creditor_user_id LEFT JOIN users debtor_user ON debtor_user.id = r.owner_user_id WHERE r.owner_user_id IN (?, ?) AND NOT (r.split_id IS NOT NULL AND r.category_id IS NULL) AND r.settle = 0 AND r.split_id IS NOT NULL AND ((r.owner_user_id = ? AND r.creditor_user_id = ?) OR (r.owner_user_id = ? AND r.creditor_user_id = ?)) ORDER BY r.date DESC, r.id DESC LIMIT ? OFFSET ?",
            (
                current_user.id.as_str(),
                friend_id.as_str(),
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

pub async fn settle_all_unsettled_splits_with_friend(
    State(app_state): State<AppState>,
    session: Session,
    Path(friend_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;

    validate_string_length(&friend_id, "Friend ID", MAX_RECORD_NAME_LENGTH)?;
    let friend_id = friend_id.trim().to_string();
    if friend_id == current_user.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Friend ID cannot be your own user ID".to_string(),
        ));
    }

    let user_id = current_user.id.clone();

    let updated_count = with_transaction(&app_state.main_db, |conn| {
        let user_id = user_id.clone();
        let friend_id = friend_id.clone();

        Box::pin(async move {
            let affected = conn
                .execute(
                    "UPDATE records SET settle = 1 WHERE owner_user_id IN (?, ?) AND NOT (split_id IS NOT NULL AND category_id IS NULL) AND settle = 0 AND split_id IS NOT NULL AND ((owner_user_id = ? AND creditor_user_id = ?) OR (owner_user_id = ? AND creditor_user_id = ?))",
                    (
                        user_id.as_str(),
                        friend_id.as_str(),
                        user_id.as_str(),
                        friend_id.as_str(),
                        friend_id.as_str(),
                        user_id.as_str(),
                    ),
                )
                .await
                .map_err(|_| TransactionError::Commit)?;

            u32::try_from(affected).map_err(|_| TransactionError::Commit)
        })
    })
    .await
    .map_err(|error| match error {
        TransactionError::Begin => db_error_with_context("failed to settle splits with friend"),
        TransactionError::Commit => {
            db_error_with_context("failed to persist settle-all split updates")
        }
    })?;

    Ok((
        StatusCode::OK,
        Json(json!({ "updated_count": updated_count })),
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
    let amount_cents: i64 = row
        .get(4)
        .map_err(|_| db_error_with_context("invalid split list amount"))?;
    let currency: String = row
        .get(5)
        .map_err(|_| db_error_with_context("invalid split list currency"))?;
    let debtor_user_id: Option<String> = row
        .get(6)
        .map_err(|_| db_error_with_context("invalid split list debtor"))?;
    let creditor_user_id: Option<String> = row
        .get(7)
        .map_err(|_| db_error_with_context("invalid split list creditor"))?;
    let creditor_name: String = row
        .get(8)
        .map_err(|_| db_error_with_context("invalid split list creditor name"))?;
    let debtor_name: String = row
        .get(9)
        .map_err(|_| db_error_with_context("invalid split list debtor name"))?;
    let pending: bool = row
        .get(10)
        .map_err(|_| db_error_with_context("invalid split list pending flag"))?;
    let settle: bool = row
        .get(11)
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
        amount: to_decimal(amount_cents.abs()),
        currency,
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
