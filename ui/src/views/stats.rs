use dioxus::prelude::*;

use crate::api;
use crate::models::{Category, Record};

fn format_amount(amount: f64) -> String {
    if amount >= 0.0 {
        format!("+{:.2}", amount)
    } else {
        format!("{:.2}", amount)
    }
}

fn get_month_name(month: u32) -> &'static str {
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

#[component]
pub fn StatsView(categories: Vec<Category>) -> Element {
    let mut records = use_signal(Vec::<Record>::new);
    let mut loading = use_signal(|| true);

    // Filters
    let mut start_date = use_signal(String::new);
    let mut end_date = use_signal(String::new);
    let mut filter_category = use_signal(String::new);
    let mut search = use_signal(String::new);

    let fetch_records = move || {
        let start_time = if start_date().is_empty() {
            None
        } else {
            chrono::NaiveDate::parse_from_str(&start_date(), "%Y-%m-%d")
                .ok()
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
        };

        let end_time = if end_date().is_empty() {
            None
        } else {
            chrono::NaiveDate::parse_from_str(&end_date(), "%Y-%m-%d")
                .ok()
                .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp())
        };

        spawn(async move {
            loading.set(true);
            if let Ok(response) = api::get_records(start_time, end_time, Some(1000), None).await {
                records.set(response.records);
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        fetch_records();
    });

    // Filter records
    let filtered_records: Vec<Record> = records()
        .into_iter()
        .filter(|r| {
            let cat_match = filter_category().is_empty() || r.category_id == filter_category();
            let search_match = search().is_empty()
                || r.name.to_lowercase().contains(&search().to_lowercase());
            cat_match && search_match
        })
        .collect();

    // Calculate stats
    let total_income: f64 = filtered_records
        .iter()
        .filter(|r| r.amount > 0.0)
        .map(|r| r.amount)
        .sum();

    let total_expense: f64 = filtered_records
        .iter()
        .filter(|r| r.amount < 0.0)
        .map(|r| r.amount.abs())
        .sum();

    let net = total_income - total_expense;

    let avg_transaction = if !filtered_records.is_empty() {
        filtered_records.iter().map(|r| r.amount.abs()).sum::<f64>() / filtered_records.len() as f64
    } else {
        0.0
    };

    // Category breakdown
    let mut category_income: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut category_expense: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for record in &filtered_records {
        if record.amount > 0.0 {
            *category_income.entry(record.category_id.clone()).or_insert(0.0) += record.amount;
        } else {
            *category_expense.entry(record.category_id.clone()).or_insert(0.0) += record.amount.abs();
        }
    }

    let max_expense = category_expense.values().cloned().fold(0.0_f64, f64::max);
    let max_income = category_income.values().cloned().fold(0.0_f64, f64::max);

    // Monthly trend (last 6 months)
    let mut monthly_data: std::collections::BTreeMap<(i32, u32), (f64, f64)> = std::collections::BTreeMap::new();
    for record in &filtered_records {
        use chrono::{Datelike, TimeZone, Utc};
        let dt = Utc.timestamp_opt(record.timestamp, 0).unwrap();
        let key = (dt.year(), dt.month());
        let entry = monthly_data.entry(key).or_insert((0.0, 0.0));
        if record.amount > 0.0 {
            entry.0 += record.amount;
        } else {
            entry.1 += record.amount.abs();
        }
    }

    let monthly_vec: Vec<_> = monthly_data.iter().rev().take(6).collect();
    let max_monthly = monthly_vec
        .iter()
        .map(|(_, (i, e))| i.max(*e))
        .fold(0.0_f64, f64::max);

    rsx! {
        div { class: "content-header",
            h1 { "STATS" }
        }

        div { class: "filters",
            div { class: "filter-group",
                label { "START DATE" }
                input {
                    r#type: "date",
                    value: "{start_date}",
                    onchange: move |e| {
                        start_date.set(e.value());
                        fetch_records();
                    }
                }
            }

            div { class: "filter-group",
                label { "END DATE" }
                input {
                    r#type: "date",
                    value: "{end_date}",
                    onchange: move |e| {
                        end_date.set(e.value());
                        fetch_records();
                    }
                }
            }

            div { class: "filter-group",
                label { "CATEGORY" }
                select {
                    value: "{filter_category}",
                    onchange: move |e| filter_category.set(e.value()),
                    option { value: "", "ALL" }
                    for cat in categories.iter() {
                        option { value: "{cat.id}", "{cat.name}" }
                    }
                }
            }

            div { class: "filter-group",
                label { "SEARCH" }
                input {
                    r#type: "text",
                    placeholder: "Search...",
                    value: "{search}",
                    oninput: move |e| search.set(e.value()),
                }
            }
        }

        if loading() {
            div { class: "loading", "LOADING..." }
        } else {
            div { class: "dashboard-grid",
                // Summary stats
                div { class: "dashboard-block",
                    h2 { "SUMMARY" }

                    div { class: "analytics-item",
                        div { class: "analytics-label", "TOTAL INCOME" }
                        div { class: "analytics-value", "{format_amount(total_income)}" }
                    }

                    div { class: "analytics-item",
                        div { class: "analytics-label", "TOTAL EXPENSE" }
                        div { class: "analytics-value", "{format_amount(-total_expense)}" }
                    }

                    div { class: "analytics-item",
                        div { class: "analytics-label", "NET" }
                        div { class: "analytics-value", "{format_amount(net)}" }
                    }

                    div { class: "analytics-item",
                        div { class: "analytics-label", "AVG TRANSACTION" }
                        div { class: "analytics-value", "{avg_transaction:.2}" }
                    }

                    div { class: "analytics-item",
                        div { class: "analytics-label", "TOTAL TRANSACTIONS" }
                        div { class: "analytics-value", "{filtered_records.len()}" }
                    }
                }

                // Monthly trend
                div { class: "dashboard-block",
                    h2 { "MONTHLY TREND" }

                    if monthly_vec.is_empty() {
                        div { class: "empty-state", "NO DATA" }
                    } else {
                        div { class: "bar-chart",
                            for ((year, month), (income, expense)) in monthly_vec.iter().rev() {
                                {
                                    let income_pct = if max_monthly > 0.0 { (income / max_monthly) * 100.0 } else { 0.0 };
                                    let expense_pct = if max_monthly > 0.0 { (expense / max_monthly) * 100.0 } else { 0.0 };
                                    rsx! {
                                        div { class: "mb-2", key: "{year}-{month}",
                                            div { class: "bar-label mb-1", "{get_month_name(*month)} {year}" }
                                            div { class: "bar-row",
                                                span { class: "bar-label", "IN" }
                                                div { class: "bar-track",
                                                    div { class: "bar-fill", style: "width: {income_pct}%" }
                                                }
                                                span { class: "bar-value", "{income:.0}" }
                                            }
                                            div { class: "bar-row",
                                                span { class: "bar-label", "OUT" }
                                                div { class: "bar-track",
                                                    div { class: "bar-fill", style: "width: {expense_pct}%" }
                                                }
                                                span { class: "bar-value", "{expense:.0}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Expense by category
                div { class: "dashboard-block",
                    h2 { "EXPENSE BY CATEGORY" }

                    if category_expense.is_empty() {
                        div { class: "empty-state", "NO EXPENSES" }
                    } else {
                        div { class: "bar-chart",
                            for (cat_id, total) in category_expense.iter() {
                                {
                                    let cat = categories.iter().find(|c| &c.id == cat_id);
                                    let cat_name = cat.map(|c| c.name.as_str()).unwrap_or("—");
                                    let pct = if max_expense > 0.0 { (total / max_expense) * 100.0 } else { 0.0 };
                                    rsx! {
                                        div { class: "bar-row", key: "{cat_id}",
                                            span { class: "bar-label", "{cat_name}" }
                                            div { class: "bar-track",
                                                div { class: "bar-fill", style: "width: {pct}%" }
                                            }
                                            span { class: "bar-value", "{total:.2}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Income by category
                div { class: "dashboard-block",
                    h2 { "INCOME BY CATEGORY" }

                    if category_income.is_empty() {
                        div { class: "empty-state", "NO INCOME" }
                    } else {
                        div { class: "bar-chart",
                            for (cat_id, total) in category_income.iter() {
                                {
                                    let cat = categories.iter().find(|c| &c.id == cat_id);
                                    let cat_name = cat.map(|c| c.name.as_str()).unwrap_or("—");
                                    let pct = if max_income > 0.0 { (total / max_income) * 100.0 } else { 0.0 };
                                    rsx! {
                                        div { class: "bar-row", key: "{cat_id}",
                                            span { class: "bar-label", "{cat_name}" }
                                            div { class: "bar-track",
                                                div { class: "bar-fill", style: "width: {pct}%" }
                                            }
                                            span { class: "bar-value", "{total:.2}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
