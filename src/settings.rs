use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{UpdateUserSettingsPayload, UserSettings};
use crate::validation::validate_currency;

#[utoipa::path(
    get,
    path = "/settings",
    tag = "settings",
    responses(
        (status = 200, description = "User settings", body = crate::models::UserSettings),
        (status = 401, description = "Not logged in"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_settings(
    State(app_state): State<AppState>,
    session: Session,
) -> Result<(StatusCode, Json<UserSettings>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    let mut rows = conn
        .query(
            "SELECT main_currency FROM users WHERE id = ?",
            [user.id.as_str()],
        )
        .await
        .inspect_err(|e| tracing::error!("failed to query user settings: {e}"))
        .map_err(|_| db_error_with_context("failed to query user settings"))?;

    if let Some(row) = rows
        .next()
        .await
        .inspect_err(|e| tracing::error!("failed to read settings row: {e}"))
        .map_err(|_| db_error())?
    {
        let main_currency: String = row
            .get(0)
            .inspect_err(|e| tracing::error!("invalid user settings data: {e}"))
            .map_err(|_| db_error_with_context("invalid user settings data"))?;

        return Ok((StatusCode::OK, Json(UserSettings { main_currency })));
    }

    Err((StatusCode::NOT_FOUND, "User not found".to_string()))
}

#[utoipa::path(
    put,
    path = "/settings",
    tag = "settings",
    request_body = crate::models::UpdateUserSettingsPayload,
    responses(
        (status = 200, description = "Updated user settings", body = crate::models::UserSettings),
        (status = 401, description = "Not logged in"),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_settings(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<UpdateUserSettingsPayload>,
) -> Result<(StatusCode, Json<UserSettings>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let main_currency = validate_currency(&payload.main_currency)?;

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    conn.execute(
        "UPDATE users SET main_currency = ? WHERE id = ?",
        (main_currency.as_str(), user.id.as_str()),
    )
    .await
    .inspect_err(|e| tracing::error!("failed to update user settings: {e}"))
    .map_err(|_| db_error_with_context("failed to update user settings"))?;

    Ok((StatusCode::OK, Json(UserSettings { main_currency })))
}
