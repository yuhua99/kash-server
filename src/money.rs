use crate::constants::MINOR_UNITS_PER_MAJOR;

pub fn to_cents(amount: f64) -> i64 {
    (amount * MINOR_UNITS_PER_MAJOR as f64).round() as i64
}

pub fn to_decimal(cents: i64) -> f64 {
    cents as f64 / MINOR_UNITS_PER_MAJOR as f64
}
