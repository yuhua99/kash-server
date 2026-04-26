use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::models::{UpdateUserSettingsPayload, UserSettings};
use crate::utils::{db_error, db_error_with_context, validate_currency};

pub async fn get_settings(
    State(app_state): State<AppState>,
    session: Session,
) -> Result<(StatusCode, Json<UserSettings>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let conn = app_state.main_db.connect().map_err(|_| db_error())?;
    let mut rows = conn
        .query(
            "SELECT main_currency FROM users WHERE id = ?",
            [user.id.as_str()],
        )
        .await
        .map_err(|_| db_error_with_context("failed to query user settings"))?;

    if let Some(row) = rows.next().await.map_err(|_| db_error())? {
        let main_currency: String = row
            .get(0)
            .map_err(|_| db_error_with_context("invalid user settings data"))?;

        return Ok((StatusCode::OK, Json(UserSettings { main_currency })));
    }

    Err((StatusCode::NOT_FOUND, "User not found".to_string()))
}

pub async fn update_settings(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<UpdateUserSettingsPayload>,
) -> Result<(StatusCode, Json<UserSettings>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    let main_currency = validate_currency(&payload.main_currency)?;

    let conn = app_state.main_db.connect().map_err(|_| db_error())?;
    conn.execute(
        "UPDATE users SET main_currency = ? WHERE id = ?",
        (main_currency.as_str(), user.id.as_str()),
    )
    .await
    .map_err(|_| db_error_with_context("failed to update user settings"))?;

    Ok((StatusCode::OK, Json(UserSettings { main_currency })))
}
