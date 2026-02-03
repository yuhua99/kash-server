mod api;
mod components;
mod models;
mod utils;
mod views;

use dioxus::prelude::*;

use components::{Section, Shell};
use models::{Category, PublicUser};
use views::{AuthScreen, CategoriesView, DashboardView, SettingsView, StatsView, TransactionsView};

fn main() {
    dioxus::launch(App);
}

#[allow(non_snake_case)]
fn App() -> Element {
    let mut user = use_signal(|| None::<PublicUser>);
    let mut current_section = use_signal(|| Section::Dashboard);
    let mut categories = use_signal(Vec::<Category>::new);
    let mut checking_auth = use_signal(|| true);

    // Check if user is already logged in
    use_effect(move || {
        spawn(async move {
            if let Ok(u) = api::get_me().await {
                user.set(Some(u));
                // Fetch categories after login
                if let Ok(response) = api::get_categories(None, None, None).await {
                    categories.set(response.categories);
                }
            }
            checking_auth.set(false);
        });
    });

    let handle_login = move |u: PublicUser| {
        user.set(Some(u));
        // Fetch categories after login
        spawn(async move {
            if let Ok(response) = api::get_categories(None, None, None).await {
                categories.set(response.categories);
            }
        });
    };

    let handle_logout = move |_| {
        spawn(async move {
            let _ = api::logout().await;
            user.set(None);
            categories.set(vec![]);
            current_section.set(Section::Dashboard);
        });
    };

    let handle_section_change = move |section: Section| {
        current_section.set(section);
    };

    let handle_categories_change = move |cats: Vec<Category>| {
        categories.set(cats);
    };

    // Show loading while checking auth
    if checking_auth() {
        return rsx! {
            div { class: "auth-container",
                div { class: "loading", "LOADING..." }
            }
        };
    }

    // Show auth screen if not logged in
    let Some(current_user) = user() else {
        return rsx! {
            AuthScreen { on_login: handle_login }
        };
    };

    // Main app shell
    rsx! {
        Shell {
            user: current_user.clone(),
            current_section: current_section(),
            on_section_change: handle_section_change,
            on_logout: handle_logout,

            match current_section() {
                Section::Dashboard => rsx! {
                    DashboardView { categories: categories() }
                },
                Section::Stats => rsx! {
                    StatsView { categories: categories() }
                },
                Section::Transactions => rsx! {
                    TransactionsView { categories: categories() }
                },
                Section::Categories => rsx! {
                    CategoriesView {
                        categories: categories(),
                        on_categories_change: handle_categories_change
                    }
                },
                Section::Settings => rsx! {
                    SettingsView {
                        username: current_user.username.clone(),
                        on_logout: handle_logout
                    }
                },
            }
        }
    }
}
