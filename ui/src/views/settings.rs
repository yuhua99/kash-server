use dioxus::prelude::*;

#[component]
pub fn SettingsView(username: String, on_logout: EventHandler<()>) -> Element {
    let mut density = use_signal(|| "comfortable".to_string());

    rsx! {
        div { class: "content-header",
            h1 { "SETTINGS" }
        }

        div { class: "settings-block",
            h2 { "ACCOUNT" }

            div { class: "settings-item",
                span { "USERNAME" }
                span { class: "font-mono", "{username}" }
            }

            div { class: "settings-item",
                span { "SESSION" }
                button { onclick: move |_| on_logout.call(()), "LOGOUT" }
            }
        }

        div { class: "settings-block mt-2",
            h2 { "PREFERENCES" }

            div { class: "settings-item",
                span { "DENSITY" }
                select {
                    value: "{density}",
                    onchange: move |e| density.set(e.value()),
                    option { value: "compact", "COMPACT" }
                    option { value: "comfortable", "COMFORTABLE" }
                }
            }
        }
    }
}
