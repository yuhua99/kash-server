use chrono::{Datelike, NaiveDate, TimeZone, Utc};

pub fn format_amount(amount: f64) -> String {
    if amount >= 0.0 {
        format!("+{:.2}", amount)
    } else {
        format!("{:.2}", amount)
    }
}

pub fn format_date_short(timestamp: i64) -> String {
    let dt = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap());
    dt.format("%m/%d").to_string()
}

pub fn format_date_full(timestamp: i64) -> String {
    let dt = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap());
    dt.format("%Y-%m-%d").to_string()
}

pub fn parse_date_to_timestamp(date_str: &str) -> Option<i64> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc().timestamp()))
}

pub fn parse_date_to_end_of_day(date_str: &str) -> Option<i64> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59).map(|dt| dt.and_utc().timestamp()))
}

pub fn get_month_name(month: u32) -> &'static str {
    match month {
        1 => "JAN",
        2 => "FEB",
        3 => "MAR",
        4 => "APR",
        5 => "MAY",
        6 => "JUN",
        7 => "JUL",
        8 => "AUG",
        9 => "SEP",
        10 => "OCT",
        11 => "NOV",
        12 => "DEC",
        _ => "???",
    }
}

pub fn format_month(year: i32, month: u32) -> String {
    format!("{} {}", get_month_name(month), year)
}

pub fn get_month_range(year: i32, month: u32) -> (i64, i64) {
    debug_assert!(
        (1..=12).contains(&month),
        "month must be between 1 and 12, got {}",
        month
    );
    let start = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .unwrap();
    let end = if month == 12 {
        Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
    } else {
        Utc.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0)
            .single()
            .unwrap()
    };
    (start.timestamp(), end.timestamp() - 1)
}

pub fn timestamp_to_year_month(timestamp: i64) -> (i32, u32) {
    let dt = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap());
    (dt.year(), dt.month())
}
