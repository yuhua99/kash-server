use dioxus::prelude::*;

use crate::api;
use crate::components::Overlay;
use crate::models::{Category, CreateRecordPayload, Record};
use crate::utils::{format_amount, format_date_short, format_month, get_month_range, today_date};

#[component]
pub fn DashboardView(categories: Vec<Category>) -> Element {
    let now = chrono::Utc::now();
    let mut current_year = use_signal(|| now.format("%Y").to_string().parse::<i32>().unwrap());
    let mut current_month = use_signal(|| now.format("%m").to_string().parse::<u32>().unwrap());

    let mut show_add_overlay = use_signal(|| false);
    let mut records = use_signal(Vec::<Record>::new);
    let mut loading = use_signal(|| true);

    let year = current_year();
    let month = current_month();

    // Fetch records for current month
    use_effect(move || {
        let (start, end) = get_month_range(year, month);
        spawn(async move {
            loading.set(true);
            if let Ok(response) = api::get_records(Some(start), Some(end), Some(5), None).await {
                records.set(response.records);
            }
            loading.set(false);
        });
    });

    let prev_month = move |_| {
        if current_month() == 1 {
            current_month.set(12);
            current_year.set(current_year() - 1);
        } else {
            current_month.set(current_month() - 1);
        }
    };

    let next_month = move |_| {
        if current_month() == 12 {
            current_month.set(1);
            current_year.set(current_year() + 1);
        } else {
            current_month.set(current_month() + 1);
        }
    };

    // Calculate analytics
    let records_list = records();
    let total_income: f64 = records_list
        .iter()
        .filter(|r| r.amount > 0.0)
        .map(|r| r.amount)
        .sum();
    let total_expense: f64 = records_list
        .iter()
        .filter(|r| r.amount < 0.0)
        .map(|r| r.amount.abs())
        .sum();

    let largest_expense = records_list
        .iter()
        .filter(|r| r.amount < 0.0)
        .min_by(|a, b| a.amount.partial_cmp(&b.amount).unwrap())
        .map(|r| format!("{}: {:.2}", r.name, r.amount.abs()))
        .unwrap_or_else(|| "—".to_string());

    let transaction_count = records_list.len();

    // Category breakdown for bar chart
    let mut category_totals: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for record in &records_list {
        if record.amount < 0.0 {
            *category_totals.entry(record.category_id.clone()).or_insert(0.0) += record.amount.abs();
        }
    }
    let max_category_total = category_totals.values().cloned().fold(0.0_f64, f64::max);

    let categories_clone = categories.clone();

    rsx! {
        div { class: "content-header",
            h1 { "DASHBOARD" }
            div { class: "flex gap-2 items-center",
                div { class: "month-selector",
                    button { onclick: prev_month, "<" }
                    span { class: "month-text", "{format_month(current_year(), current_month())}" }
                    button { onclick: next_month, ">" }
                }
                button { onclick: move |_| show_add_overlay.set(true), "ADD" }
            }
        }

        div { class: "dashboard-grid",
            div { class: "dashboard-block",
                h2 { "RECENT TRANSACTIONS" }
                if loading() {
                    div { class: "loading", "LOADING..." }
                } else if records().is_empty() {
                    div { class: "empty-state", "NO TRANSACTIONS" }
                } else {
                    div { class: "transaction-list",
                        for record in records() {
                            {
                                let cat = categories_clone.iter().find(|c| c.id == record.category_id);
                                let cat_name = cat.map(|c| c.name.as_str()).unwrap_or("—");
                                rsx! {
                                    div { class: "transaction-row", key: "{record.id}",
                                        span { class: "date", "{format_date_short(&record.date)}" }
                                        span { class: "name", "{record.name}" }
                                        span { class: "category", "{cat_name}" }
                                        span {
                                            class: if record.amount >= 0.0 { "amount income" } else { "amount expense" },
                                            "{format_amount(record.amount)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "dashboard-block",
                h2 { "ANALYTICS SNAPSHOT" }

                div { class: "analytics-item",
                    div { class: "analytics-label", "TOTAL INCOME" }
                    div { class: "analytics-value", "+{total_income:.2}" }
                }

                div { class: "analytics-item",
                    div { class: "analytics-label", "TOTAL EXPENSE" }
                    div { class: "analytics-value", "-{total_expense:.2}" }
                }

                div { class: "analytics-item",
                    div { class: "analytics-label", "LARGEST EXPENSE" }
                    div { class: "analytics-value", "{largest_expense}" }
                }

                div { class: "analytics-item",
                    div { class: "analytics-label", "TRANSACTIONS" }
                    div { class: "analytics-value", "{transaction_count}" }
                }

                if !category_totals.is_empty() {
                    div { class: "analytics-item mt-2",
                        div { class: "analytics-label mb-1", "EXPENSE BY CATEGORY" }
                        div { class: "bar-chart",
                            for (cat_id, total) in category_totals.iter() {
                                {
                                    let cat = categories.iter().find(|c| &c.id == cat_id);
                                    let cat_name = cat.map(|c| c.name.as_str()).unwrap_or("—");
                                    let pct = if max_category_total > 0.0 { (total / max_category_total) * 100.0 } else { 0.0 };
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

        if show_add_overlay() {
            AddTransactionOverlay {
                categories: categories.clone(),
                on_close: move |_| show_add_overlay.set(false),
                on_save: move |record| {
                    let mut current = records();
                    current.insert(0, record);
                    if current.len() > 5 {
                        current.pop();
                    }
                    records.set(current);
                    show_add_overlay.set(false);
                }
            }
        }
    }
}

#[component]
fn AddTransactionOverlay(
    categories: Vec<Category>,
    on_close: EventHandler<()>,
    on_save: EventHandler<Record>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut amount = use_signal(String::new);
    let mut category_id = use_signal(String::new);
    let mut is_income_type = use_signal(|| false);
    let mut date = use_signal(today_date);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    // Set default category
    use_effect({
        let categories = categories.clone();
        move || {
            if category_id().is_empty() && !categories.is_empty() {
                is_income_type.set(categories[0].is_income);
                category_id.set(categories[0].id.clone());
            }
        }
    });

    let categories_for_type_change = categories.clone();
    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        e.stop_propagation();

        let name_val = name().trim().to_string();
        let amount_val: f64 = match amount().parse() {
            Ok(v) => v,
            Err(_) => {
                error.set(Some("Invalid amount".to_string()));
                return;
            }
        };
        let cat_id = category_id();

        if name_val.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        if cat_id.is_empty() {
            error.set(Some("Category is required".to_string()));
            return;
        }

        let final_amount = if is_income_type() {
            amount_val.abs()
        } else {
            -amount_val.abs()
        };

        loading.set(true);
        error.set(None);

        spawn(async move {
            let result = api::create_record(CreateRecordPayload {
                name: name_val,
                amount: final_amount,
                category_id: cat_id,
                date: date(),
            })
            .await;

            loading.set(false);

            match result {
                Ok(record) => {
                    on_save.call(record);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
        });
    };

    let filtered_categories: Vec<Category> = categories
        .iter()
        .filter(|cat| cat.is_income == is_income_type())
        .cloned()
        .collect();

    rsx! {
        Overlay { title: "ADD TRANSACTION".to_string(), on_close: on_close,
            if let Some(err) = error() {
                div { class: "error-message", "{err}" }
            }

            form { onsubmit: handle_submit,
                div { class: "form-group",
                    label { "AMOUNT" }
                    input {
                        r#type: "number",
                        step: "0.01",
                        value: "{amount}",
                        oninput: move |e| amount.set(e.value()),
                        disabled: loading(),
                    }
                }

                div { class: "form-group",
                    label { "TYPE" }
                    select {
                        value: if is_income_type() { "income" } else { "expense" },
                        onchange: move |e| {
                            let next_is_income = e.value() == "income";
                            is_income_type.set(next_is_income);
                            if let Some(cat) = categories_for_type_change
                                .iter()
                                .find(|c| c.is_income == next_is_income)
                            {
                                category_id.set(cat.id.clone());
                            }
                        },
                        disabled: loading(),
                        option { value: "expense", "EXPENSE" }
                        option { value: "income", "INCOME" }
                    }
                }

                div { class: "form-group",
                    label { "CATEGORY" }
                    select {
                        value: "{category_id}",
                        onchange: move |e| category_id.set(e.value()),
                        disabled: loading(),
                        for cat in filtered_categories.iter() {
                            option { value: "{cat.id}", "{cat.name}" }
                        }
                    }
                }

                div { class: "form-group",
                    label { "DATE" }
                    input {
                        r#type: "date",
                        value: "{date}",
                        onchange: move |e| date.set(e.value()),
                        disabled: loading(),
                    }
                }

                div { class: "form-group",
                    label { "NAME" }
                    input {
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                        disabled: loading(),
                    }
                }

                button {
                    class: "primary w-full",
                    r#type: "submit",
                    disabled: loading(),
                    if loading() { "SAVING..." } else { "SAVE" }
                }
            }
        }
    }
}
