use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::auth::get_current_user;
use crate::errors::db_error_with_context;
use crate::validation::validate_string_length;
use crate::{AppState, TransactionError, with_transaction};

enum RevokeSplitError {
    Transaction(TransactionError),
    Db,
    NotFound,
    Settled,
}

impl From<TransactionError> for RevokeSplitError {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error)
    }
}

impl From<libsql::Error> for RevokeSplitError {
    fn from(error: libsql::Error) -> Self {
        tracing::error!("failed to revoke split: {error}");
        Self::Db
    }
}

#[utoipa::path(
    delete,
    path = "/splits/{id}",
    tag = "splits",
    params(("id" = String, Path, description = "Split id")),
    responses(
        (status = 200, description = "Split revoked; existing records retained"),
        (status = 400, description = "Invalid input"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Split not found"),
        (status = 409, description = "A share has already been settled"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn revoke_split(
    State(app_state): State<AppState>,
    session: Session,
    Path(split_id): Path<String>,
) -> Result<(StatusCode, Json<()>), (StatusCode, String)> {
    let current_user = get_current_user(&session).await?;
    validate_string_length(
        &split_id,
        "Split ID",
        crate::constants::MAX_RECORD_NAME_LENGTH,
    )?;

    with_transaction(&app_state.main_db, |conn| {
        Box::pin(async move {
            let settled = {
                let mut rows = conn
                    .query(
                        "SELECT EXISTS (SELECT 1 FROM split_participants sp WHERE sp.split_id = s.id AND sp.settled = 1) FROM splits s WHERE s.id = ? AND s.creditor_user_id = ?",
                        (split_id.as_str(), current_user.id.as_str()),
                    )
                    .await?;
                let row = rows.next().await?.ok_or(RevokeSplitError::NotFound)?;
                row.get::<bool>(0)?
            };
            if settled {
                return Err(RevokeSplitError::Settled);
            }
            // Participants cascade away; neither side's independent records are deleted.
            conn.execute(
                "DELETE FROM splits WHERE id = ? AND creditor_user_id = ?",
                (split_id.as_str(), current_user.id.as_str()),
            )
            .await?;
            Ok(())
        })
    })
    .await
    .map_err(|error| match error {
        RevokeSplitError::NotFound => (StatusCode::NOT_FOUND, "Split not found".to_string()),
        RevokeSplitError::Settled => (
            StatusCode::CONFLICT,
            "Cannot revoke a split with settled shares".to_string(),
        ),
        RevokeSplitError::Transaction(TransactionError::Begin) => {
            db_error_with_context("failed to begin revoke split transaction")
        }
        RevokeSplitError::Transaction(TransactionError::Commit) => {
            db_error_with_context("failed to commit revoke split transaction")
        }
        RevokeSplitError::Db => db_error_with_context("failed to revoke split"),
    })?;

    Ok((StatusCode::OK, Json(())))
}
