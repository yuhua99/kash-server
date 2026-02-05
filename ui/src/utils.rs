use chrono::{Datelike, NaiveDate, Utc};

pub fn format_amount(amount: f64) -> String {
    if amount >= 0.0 {
        format!("+{:.2}", amount)
    } else {
        format!("{:.2}", amount)
    }
}

fn parse_date(date_str: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
}

pub fn format_date_short(date_str: &str) -> String {
    parse_date(date_str)
        .map(|d| d.format("%m/%d").to_string())
        .unwrap_or_else(|| date_str.to_string())
}

pub fn format_date_full(date_str: &str) -> String {
    parse_date(date_str)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| date_str.to_string())
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

pub fn get_month_range(year: i32, month: u32) -> (String, String) {
    debug_assert!(
        (1..=12).contains(&month),
        "month must be between 1 and 12, got {}",
        month
    );
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .pred_opt()
        .unwrap();
    (
        start.format("%Y-%m-%d").to_string(),
        end.format("%Y-%m-%d").to_string(),
    )
}

pub fn date_to_year_month(date_str: &str) -> Option<(i32, u32)> {
    parse_date(date_str).map(|d| (d.year(), d.month()))
}

pub fn today_date() -> String {
    Utc::now().date_naive().format("%Y-%m-%d").to_string()
}
