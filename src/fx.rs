use std::collections::HashMap;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use reqwest::Client;
use serde::Deserialize;
use tower_sessions::Session;

use crate::AppState;
use crate::auth::get_current_user;
use crate::constants::FX_ANCHOR_BASE_CURRENCY;
use crate::errors::{db_error, db_error_with_context};
use crate::models::{ExchangeRateRow, GetFxRatesQuery, GetFxRatesResponse};
use crate::validation::{validate_currency_list, validate_date};

const FRANKFURTER_RATES_URL: &str = "https://api.frankfurter.dev/v2/rates";

#[derive(Deserialize)]
struct FrankfurterRateRow {
    date: String,
    quote: String,
    rate: f64,
}

#[utoipa::path(
    get,
    path = "/fx/rates",
    tag = "fx",
    params(GetFxRatesQuery),
    responses(
        (status = 200, description = "FX rates", body = crate::models::GetFxRatesResponse),
        (status = 401, description = "Not logged in"),
        (status = 400, description = "Invalid input"),
        (status = 502, description = "Upstream FX provider error"),
        (status = 500, description = "Internal server error")
    )
)]
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

    let currencies = validate_currency_list(&query.quotes)?;
    let dates = enumerate_dates(&query.from, &query.to)?;
    let lookup_currencies: Vec<String> = currencies
        .iter()
        .filter(|currency| currency.as_str() != FX_ANCHOR_BASE_CURRENCY)
        .cloned()
        .collect();

    if !lookup_currencies.is_empty() {
        let cached_rates =
            load_cached_rates(&app_state, &query.from, &query.to, &lookup_currencies).await?;
        if is_missing_any_rate(&cached_rates, &dates, &lookup_currencies) {
            let fetched_rates =
                fetch_frankfurter_rates(&query.from, &query.to, &lookup_currencies).await?;
            upsert_exchange_rates(&app_state, &fetched_rates).await?;
        }
    }

    let mut rates =
        load_cached_rates(&app_state, &query.from, &query.to, &lookup_currencies).await?;
    let mut pre_range_rates = HashMap::new();
    if let Some(first_date) = dates.first() {
        let available_on_first_date = rates
            .iter()
            .filter(|row| row.date == *first_date)
            .map(|row| row.currency.as_str())
            .collect::<std::collections::HashSet<_>>();

        for currency in &lookup_currencies {
            if available_on_first_date.contains(currency.as_str()) {
                continue;
            }

            if let Some(rate) = load_latest_rate_before(&app_state, first_date, currency).await? {
                pre_range_rates.insert(currency.clone(), rate);
            }
        }
    }

    append_usd_identity_rates(&mut rates, &dates, &currencies);
    forward_fill_missing_rates(&mut rates, &dates, &currencies, &pre_range_rates);

    Ok((StatusCode::OK, Json(GetFxRatesResponse { rates })))
}

fn append_usd_identity_rates(
    rates: &mut Vec<ExchangeRateRow>,
    dates: &[String],
    currencies: &[String],
) {
    if !currencies
        .iter()
        .any(|currency| currency == FX_ANCHOR_BASE_CURRENCY)
    {
        return;
    }

    rates.extend(dates.iter().cloned().map(|date| ExchangeRateRow {
        date,
        currency: FX_ANCHOR_BASE_CURRENCY.to_string(),
        rate: 1.0,
    }));
    rates.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.currency.cmp(&right.currency))
    });
}

fn forward_fill_missing_rates(
    rates: &mut Vec<ExchangeRateRow>,
    dates: &[String],
    currencies: &[String],
    pre_range_rates: &HashMap<String, f64>,
) {
    let mut known_rates = rates
        .iter()
        .map(|row| ((row.date.clone(), row.currency.clone()), row.rate))
        .collect::<HashMap<_, _>>();
    let mut synthetic_rows = Vec::new();

    for currency in currencies {
        let mut last_known_rate = pre_range_rates.get(currency).copied();

        for date in dates {
            let key = (date.clone(), currency.clone());
            if let Some(rate) = known_rates.get(&key) {
                last_known_rate = Some(*rate);
                continue;
            }

            if let Some(rate) = last_known_rate {
                synthetic_rows.push(ExchangeRateRow {
                    date: date.clone(),
                    currency: currency.clone(),
                    rate,
                });
                known_rates.insert(key, rate);
            }
        }
    }

    rates.extend(synthetic_rows);
    rates.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.currency.cmp(&right.currency))
    });
}

async fn load_cached_rates(
    app_state: &AppState,
    from: &str,
    to: &str,
    currencies: &[String],
) -> Result<Vec<ExchangeRateRow>, (StatusCode, String)> {
    if currencies.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat_n("?", currencies.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT date, currency, rate FROM exchange_rates_daily WHERE currency IN ({}) AND date BETWEEN ? AND ? ORDER BY date ASC, currency ASC",
        placeholders
    );

    let mut params: Vec<libsql::Value> = Vec::with_capacity(currencies.len() + 2);
    params.extend(currencies.iter().cloned().map(libsql::Value::from));
    params.push(from.to_string().into());
    params.push(to.to_string().into());

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    let mut rows = conn
        .query(&sql, params)
        .await
        .inspect_err(|e| tracing::error!("failed to query exchange rate cache: {e}"))
        .map_err(|_| db_error_with_context("failed to query exchange rate cache"))?;

    let mut rates = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .inspect_err(|e| tracing::error!("failed to read fx row: {e}"))
        .map_err(|_| db_error())?
    {
        rates.push(ExchangeRateRow {
            date: row
                .get(0)
                .inspect_err(|e| tracing::error!("invalid fx rate date: {e}"))
                .map_err(|_| db_error_with_context("invalid fx rate date"))?,
            currency: row
                .get(1)
                .inspect_err(|e| tracing::error!("invalid fx currency: {e}"))
                .map_err(|_| db_error_with_context("invalid fx currency"))?,
            rate: row
                .get(2)
                .inspect_err(|e| tracing::error!("invalid fx rate value: {e}"))
                .map_err(|_| db_error_with_context("invalid fx rate value"))?,
        });
    }

    Ok(rates)
}

async fn load_latest_rate_before(
    app_state: &AppState,
    date: &str,
    currency: &str,
) -> Result<Option<f64>, (StatusCode, String)> {
    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    let mut rows = conn
        .query(
            "SELECT rate FROM exchange_rates_daily WHERE currency = ? AND date < ? ORDER BY date DESC LIMIT 1",
            (currency, date),
        )
        .await
        .inspect_err(|e| tracing::error!("failed to query latest pre-range exchange rate: {e}"))
        .map_err(|_| db_error_with_context("failed to query latest pre-range exchange rate"))?;

    let row = rows
        .next()
        .await
        .inspect_err(|e| tracing::error!("failed to read pre-range fx row: {e}"))
        .map_err(|_| db_error())?;
    row.map(|r| {
        r.get(0)
            .inspect_err(|e| tracing::error!("invalid pre-range fx rate value: {e}"))
            .map_err(|_| db_error_with_context("invalid pre-range fx rate value"))
    })
    .transpose()
}

fn is_missing_any_rate(
    cached_rates: &[ExchangeRateRow],
    dates: &[String],
    currencies: &[String],
) -> bool {
    let Some(first_weekday) = first_weekday_in_range(dates) else {
        return false;
    };

    let available = cached_rates
        .iter()
        .map(|row| (row.date.as_str(), row.currency.as_str()))
        .collect::<std::collections::HashSet<_>>();

    currencies
        .iter()
        .any(|currency| !available.contains(&(first_weekday, currency.as_str())))
}

fn first_weekday_in_range(dates: &[String]) -> Option<&str> {
    let format = time::format_description::parse("[year]-[month]-[day]").ok()?;

    dates.iter().find_map(|date| {
        time::Date::parse(date, &format)
            .ok()
            .filter(|parsed| {
                !matches!(
                    parsed.weekday(),
                    time::Weekday::Saturday | time::Weekday::Sunday
                )
            })
            .map(|_| date.as_str())
    })
}

async fn fetch_frankfurter_rates(
    from: &str,
    to: &str,
    currencies: &[String],
) -> Result<Vec<ExchangeRateRow>, (StatusCode, String)> {
    let client = Client::new();
    let quotes = currencies.join(",");
    let response = client
        .get(FRANKFURTER_RATES_URL)
        .query(&[
            ("from", from),
            ("to", to),
            ("base", FX_ANCHOR_BASE_CURRENCY),
            ("quotes", quotes.as_str()),
        ])
        .send()
        .await
        .map_err(|_| db_error_with_context("failed to fetch exchange rates from Frankfurter"))?;

    if !response.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Frankfurter returned {}", response.status()),
        ));
    }

    let rows: Vec<FrankfurterRateRow> = response
        .json()
        .await
        .map_err(|_| db_error_with_context("failed to decode Frankfurter response"))?;

    Ok(rows
        .into_iter()
        .map(|row| ExchangeRateRow {
            date: row.date,
            currency: row.quote,
            rate: row.rate,
        })
        .collect())
}

async fn upsert_exchange_rates(
    app_state: &AppState,
    rates: &[ExchangeRateRow],
) -> Result<(), (StatusCode, String)> {
    if rates.is_empty() {
        return Ok(());
    }

    let conn = crate::database::db_conn(&app_state.main_db)
        .await
        .inspect_err(|e| tracing::error!("db connection failed: {e}"))
        .map_err(|_| db_error())?;
    for rate in rates {
        conn.execute(
            "INSERT INTO exchange_rates_daily (date, currency, rate) VALUES (?, ?, ?) ON CONFLICT(date, currency) DO UPDATE SET rate = excluded.rate",
            (rate.date.as_str(), rate.currency.as_str(), rate.rate),
        )
        .await
        .inspect_err(|e| tracing::error!("failed to store exchange rate cache: {e}"))
        .map_err(|_| db_error_with_context("failed to store exchange rate cache"))?;
    }

    Ok(())
}

fn enumerate_dates(from: &str, to: &str) -> Result<Vec<String>, (StatusCode, String)> {
    let format = time::format_description::parse("[year]-[month]-[day]").map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid date parser".to_string(),
        )
    })?;

    let mut current = time::Date::parse(from, &format)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid from date".to_string()))?;
    let end = time::Date::parse(to, &format)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid to date".to_string()))?;

    let mut dates = Vec::new();
    while current <= end {
        dates.push(current.format(&format).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid date format".to_string(),
            )
        })?);
        current = current.next_day().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to advance date".to_string(),
        ))?;
    }

    Ok(dates)
}
