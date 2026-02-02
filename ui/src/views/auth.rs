use dioxus::prelude::*;

use crate::api;
use crate::models::{LoginPayload, PublicUser, RegisterPayload};

#[derive(Clone, Copy, PartialEq)]
enum AuthMode {
    Login,
    Register,
}

#[component]
pub fn AuthScreen(on_login: EventHandler<PublicUser>) -> Element {
    let mut mode = use_signal(|| AuthMode::Login);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        e.stop_propagation();

        let username_val = username().trim().to_string();
        let password_val = password().clone();

        if username_val.is_empty() || password_val.is_empty() {
            error.set(Some("Please fill in all fields".to_string()));
            return;
        }

        loading.set(true);
        error.set(None);

        let current_mode = mode();
        spawn(async move {
            let result = match current_mode {
                AuthMode::Login => {
                    api::login(LoginPayload {
                        username: username_val,
                        password: password_val,
                    })
                    .await
                }
                AuthMode::Register => {
                    api::register(RegisterPayload {
                        username: username_val,
                        password: password_val,
                    })
                    .await
                }
            };

            loading.set(false);

            match result {
                Ok(user) => {
                    on_login.call(user);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
        });
    };

    let title = match mode() {
        AuthMode::Login => "LOGIN",
        AuthMode::Register => "REGISTER",
    };

    let switch_text = match mode() {
        AuthMode::Login => "SWITCH TO REGISTER",
        AuthMode::Register => "SWITCH TO LOGIN",
    };

    rsx! {
        div { class: "auth-container",
            div { class: "auth-block",
                h1 { "{title}" }

                if let Some(err) = error() {
                    div { class: "error-message", "{err}" }
                }

                form {
                    onsubmit: handle_submit,

                    div { class: "form-group",
                        label { "USERNAME" }
                        input {
                            r#type: "text",
                            value: "{username}",
                            oninput: move |e| username.set(e.value()),
                            disabled: loading(),
                        }
                    }

                    div { class: "form-group",
                        label { "PASSWORD" }
                        input {
                            r#type: "password",
                            value: "{password}",
                            oninput: move |e| password.set(e.value()),
                            disabled: loading(),
                        }
                    }

                    button {
                        class: "primary w-full",
                        r#type: "submit",
                        disabled: loading(),
                        if loading() { "LOADING..." } else { "{title}" }
                    }
                }

                div { class: "auth-switch",
                    button {
                        class: "btn-text",
                        onclick: move |_| {
                            mode.set(match mode() {
                                AuthMode::Login => AuthMode::Register,
                                AuthMode::Register => AuthMode::Login,
                            });
                            error.set(None);
                        },
                        "{switch_text}"
                    }
                }
            }
        }
    }
}
