use dioxus::prelude::*;

use crate::api;
use crate::components::Overlay;
use crate::models::{Category, CreateCategoryPayload, UpdateCategoryPayload};

#[component]
pub fn CategoriesView(
    categories: Vec<Category>,
    on_categories_change: EventHandler<Vec<Category>>,
) -> Element {
    let mut show_add_overlay = use_signal(|| false);
    let mut editing_category = use_signal(|| None::<Category>);

    rsx! {
        div { class: "content-header",
            h1 { "CATEGORIES" }
            button { onclick: move |_| show_add_overlay.set(true), "ADD" }
        }

        if categories.is_empty() {
            div { class: "empty-state", "NO CATEGORIES" }
        } else {
            div { class: "category-list border p-2",
                for cat in categories.iter() {
                    {
                        let cat_clone = cat.clone();
                        rsx! {
                            div {
                                class: "category-row",
                                key: "{cat.id}",
                                onclick: move |_| editing_category.set(Some(cat_clone.clone())),
                                span { class: "name", "{cat.name}" }
                                span { class: "type-tag",
                                    if cat.is_income { "INCOME" } else { "EXPENSE" }
                                }
                            }
                        }
                    }
                }
            }
        }

        if show_add_overlay() {
            {
                let categories = categories.clone();
                rsx! {
                    AddCategoryOverlay {
                        on_close: move |_| show_add_overlay.set(false),
                        on_save: move |cat| {
                            let mut current = categories.clone();
                            current.push(cat);
                            current.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                            on_categories_change.call(current);
                            show_add_overlay.set(false);
                        }
                    }
                }
            }
        }

        if let Some(cat) = editing_category() {
            {
                let categories_for_save = categories.clone();
                let categories_for_delete = categories.clone();
                rsx! {
                    EditCategoryOverlay {
                        category: cat.clone(),
                        on_close: move |_| editing_category.set(None),
                        on_save: move |updated: Category| {
                            let mut current: Vec<Category> = categories_for_save
                                .iter()
                                .map(|c| if c.id == updated.id { updated.clone() } else { c.clone() })
                                .collect();
                            current.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                            on_categories_change.call(current);
                            editing_category.set(None);
                        },
                        on_delete: move |id: String| {
                            let current: Vec<Category> = categories_for_delete
                                .iter()
                                .filter(|c| c.id != id)
                                .cloned()
                                .collect();
                            on_categories_change.call(current);
                            editing_category.set(None);
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddCategoryOverlay(on_close: EventHandler<()>, on_save: EventHandler<Category>) -> Element {
    let mut name = use_signal(String::new);
    let mut is_income = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        e.stop_propagation();

        let name_val = name().trim().to_string();

        if name_val.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        loading.set(true);
        error.set(None);

        let is_income_val = is_income();
        spawn(async move {
            let result = api::create_category(CreateCategoryPayload {
                name: name_val,
                is_income: is_income_val,
            })
            .await;

            loading.set(false);

            match result {
                Ok(cat) => on_save.call(cat),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    rsx! {
        Overlay { title: "ADD CATEGORY".to_string(), on_close: on_close,
            if let Some(err) = error() {
                div { class: "error-message", "{err}" }
            }

            form { onsubmit: handle_submit,
                div { class: "form-group",
                    label { "NAME" }
                    input {
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                        disabled: loading(),
                    }
                }

                div { class: "form-group",
                    label { "TYPE" }
                    select {
                        value: if is_income() { "income" } else { "expense" },
                        onchange: move |e| is_income.set(e.value() == "income"),
                        disabled: loading(),
                        option { value: "expense", "EXPENSE" }
                        option { value: "income", "INCOME" }
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
fn EditCategoryOverlay(
    category: Category,
    on_close: EventHandler<()>,
    on_save: EventHandler<Category>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut name = use_signal(|| category.name.clone());
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let category_id = category.id.clone();
    let category_id_for_delete = category.id.clone();
    let is_income = category.is_income;

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        e.stop_propagation();

        let name_val = name().trim().to_string();

        if name_val.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        loading.set(true);
        error.set(None);

        let id = category_id.clone();
        spawn(async move {
            let result = api::update_category(
                &id,
                UpdateCategoryPayload {
                    name: Some(name_val.clone()),
                },
            )
            .await;

            loading.set(false);

            match result {
                Ok(cat) => on_save.call(cat),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let handle_delete = move |_| {
        loading.set(true);
        let id = category_id_for_delete.clone();
        spawn(async move {
            match api::delete_category(&id).await {
                Ok(_) => on_delete.call(id),
                Err(e) => {
                    loading.set(false);
                    error.set(Some(e));
                }
            }
        });
    };

    rsx! {
        Overlay { title: "EDIT CATEGORY".to_string(), on_close: on_close,
            if let Some(err) = error() {
                div { class: "error-message", "{err}" }
            }

            form { onsubmit: handle_submit,
                div { class: "form-group",
                    label { "NAME" }
                    input {
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                        disabled: loading(),
                    }
                }

                div { class: "form-group",
                    label { "TYPE" }
                    div { class: "text-sm font-mono",
                        if is_income { "INCOME" } else { "EXPENSE" }
                        " (cannot be changed)"
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
