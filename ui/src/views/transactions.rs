use dioxus::prelude::*;

use crate::api;
use crate::components::Overlay;
use crate::models::{Category, CreateRecordPayload, Record, UpdateRecordPayload};
use crate::utils::{format_amount, format_date_full, today_date};

#[component]
pub fn TransactionsView(categories: Vec<Category>) -> Element {
    let mut records = use_signal(Vec::<Record>::new);
    let mut loading = use_signal(|| true);
    let mut total_count = use_signal(|| 0u32);

    // Filters
    let mut start_date = use_signal(String::new);
    let mut end_date = use_signal(String::new);
    let mut filter_category = use_signal(String::new);
    let mut search = use_signal(String::new);

    // Overlay state
    let mut show_add_overlay = use_signal(|| false);
    let mut editing_record = use_signal(|| None::<Record>);

    let fetch_records = move || {
        let start_date_value = if start_date().is_empty() {
            None
        } else {
            Some(start_date())
        };

        let end_date_value = if end_date().is_empty() {
            None
        } else {
            Some(end_date())
        };

        spawn(async move {
            loading.set(true);
            if let Ok(response) =
                api::get_records(start_date_value, end_date_value, Some(100), None).await
            {
                records.set(response.records);
                total_count.set(response.total_count);
            }
            loading.set(false);
        });
    };

    // Initial fetch
    use_effect(move || {
        fetch_records();
    });

    // Filter records client-side for category and search
    let filtered_records: Vec<Record> = records()
        .into_iter()
        .filter(|r| {
            let cat_match = filter_category().is_empty() || r.category_id == filter_category();
            let search_match = search().is_empty()
                || r.name.to_lowercase().contains(&search().to_lowercase());
            cat_match && search_match
        })
        .collect();

    let categories_clone = categories.clone();
    let categories_for_add = categories.clone();
    let categories_for_edit = categories.clone();

    rsx! {
        div { class: "content-header",
            h1 { "TRANSACTIONS" }
            button { onclick: move |_| show_add_overlay.set(true), "ADD" }
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
        } else if filtered_records.is_empty() {
            div { class: "empty-state", "NO TRANSACTIONS" }
        } else {
            div { class: "transaction-list border p-2",
                for record in filtered_records {
                    {
                        let cat = categories_clone.iter().find(|c| c.id == record.category_id);
                        let cat_name = cat.map(|c| c.name.as_str()).unwrap_or("—");
                        let record_clone = record.clone();
                        rsx! {
                            div {
                                class: "transaction-row",
                                key: "{record.id}",
                                onclick: move |_| editing_record.set(Some(record_clone.clone())),
                                span { class: "date", "{format_date_full(&record.date)}" }
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

        if show_add_overlay() {
            AddTransactionOverlay {
                categories: categories_for_add.clone(),
                on_close: move |_| show_add_overlay.set(false),
                on_save: move |record| {
                    let mut current = records();
                    current.insert(0, record);
                    records.set(current);
                    total_count.set(total_count() + 1);
                    show_add_overlay.set(false);
                }
            }
        }

        if let Some(record) = editing_record() {
            EditTransactionOverlay {
                record: record.clone(),
                categories: categories_for_edit.clone(),
                on_close: move |_| editing_record.set(None),
                on_save: move |updated: Record| {
                    let current: Vec<Record> = records()
                        .into_iter()
                        .map(|r| if r.id == updated.id { updated.clone() } else { r })
                        .collect();
                    records.set(current);
                    editing_record.set(None);
                },
                on_delete: move |id: String| {
                    let current: Vec<Record> = records()
                        .into_iter()
                        .filter(|r| r.id != id)
                        .collect();
                    records.set(current);
                    total_count.set(total_count().saturating_sub(1));
                    editing_record.set(None);
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
                Ok(record) => on_save.call(record),
                Err(e) => error.set(Some(e)),
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

#[component]
fn EditTransactionOverlay(
    record: Record,
    categories: Vec<Category>,
    on_close: EventHandler<()>,
    on_save: EventHandler<Record>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut name = use_signal(|| record.name.clone());
    let mut amount = use_signal(|| record.amount.abs().to_string());
    let mut category_id = use_signal(|| record.category_id.clone());
    let mut is_income_type = use_signal(|| {
        categories
            .iter()
            .find(|c| c.id == record.category_id)
            .map(|c| c.is_income)
            .unwrap_or(record.amount >= 0.0)
    });
    let mut date = use_signal(|| record.date.clone());
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let record_id = record.id.clone();
    let record_id_for_delete = record.id.clone();

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

        let final_amount = if is_income_type() {
            amount_val.abs()
        } else {
            -amount_val.abs()
        };

        loading.set(true);
        error.set(None);

        let id = record_id.clone();
        spawn(async move {
            let result = api::update_record(
                &id,
                UpdateRecordPayload {
                    name: Some(name_val),
                    amount: Some(final_amount),
                    category_id: Some(cat_id),
                    date: Some(date()),
                },
            )
            .await;

            loading.set(false);

            match result {
                Ok(record) => on_save.call(record),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let handle_delete = move |_| {
        loading.set(true);
        let id = record_id_for_delete.clone();
        spawn(async move {
            if api::delete_record(&id).await.is_ok() {
                on_delete.call(id);
            } else {
                loading.set(false);
                error.set(Some("Failed to delete".to_string()));
            }
        });
    };

    let filtered_categories: Vec<Category> = categories
        .iter()
        .filter(|cat| cat.is_income == is_income_type())
        .cloned()
        .collect();

    rsx! {
        Overlay { title: "EDIT TRANSACTION".to_string(), on_close: on_close,
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

                div { class: "flex gap-2",
                    button {
                        class: "primary flex-1",
                        r#type: "submit",
                        disabled: loading(),
                        if loading() { "SAVING..." } else { "SAVE" }
                    }
                    button {
                        r#type: "button",
                        onclick: handle_delete,
                        disabled: loading(),
                        "DELETE"
                    }
                }
            }
        }
    }
}
