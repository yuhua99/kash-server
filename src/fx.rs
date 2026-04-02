use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::models::{ExchangeRateRow, GetFxRatesQuery, GetFxRatesResponse};
use crate::utils::{db_error, db_error_with_context, validate_currency_code_list, validate_date};

pub async fn get_fx_rates(
    State(app_state): State<AppState>,
    session: Session,
    Query(query): Query<GetFxRatesQuery>,
) -> Result<(StatusCode, Json<GetFxRatesResponse>), (StatusCode, String)> {
    let _user = get_current_user(&session).await?;

    validate_date(&query.from)?;
    validate_date(&query.to)?;
    if query.from > query.to {
        return Err((
            StatusCode::BAD_REQUEST,
            "from date cannot be after to date".to_string(),
        ));
    }

    let currencies = validate_currency_code_list(&query.quotes)?;

    let placeholders = std::iter::repeat_n("?", currencies.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT date, currency, rate FROM exchange_rates_daily WHERE currency IN ({}) AND date BETWEEN ? AND ? ORDER BY date ASC, currency ASC",
        placeholders
    );

    let mut params: Vec<libsql::Value> = Vec::with_capacity(currencies.len() + 2);
    params.extend(currencies.iter().cloned().map(libsql::Value::from));
    params.push(query.from.clone().into());
    params.push(query.to.clone().into());

    let conn = app_state.main_db.read().await;
    let mut rows = conn
        .query(&sql, params)
        .await
        .map_err(|_| db_error_with_context("failed to query exchange rate cache"))?;

    let mut rates = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| db_error())? {
        rates.push(ExchangeRateRow {
            date: row
                .get(0)
                .map_err(|_| db_error_with_context("invalid fx rate date"))?,
            currency: row
                .get(1)
                .map_err(|_| db_error_with_context("invalid fx currency"))?,
            rate: row
                .get(2)
                .map_err(|_| db_error_with_context("invalid fx rate value"))?,
        });
    }

    Ok((StatusCode::OK, Json(GetFxRatesResponse { rates })))
}
