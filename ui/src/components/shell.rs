use dioxus::prelude::*;

use crate::models::PublicUser;

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    Dashboard,
    Stats,
    Transactions,
    Categories,
    Settings,
}

impl Section {
    pub fn label(&self) -> &'static str {
        match self {
            Section::Dashboard => "DASHBOARD",
            Section::Stats => "STATS",
            Section::Transactions => "TRANSACTIONS",
            Section::Categories => "CATEGORIES",
            Section::Settings => "SETTINGS",
        }
    }

    pub fn all() -> &'static [Section] {
        &[
            Section::Dashboard,
            Section::Stats,
            Section::Transactions,
            Section::Categories,
            Section::Settings,
        ]
    }
}

#[component]
pub fn TopStrip(user: PublicUser, on_logout: EventHandler<()>) -> Element {
    rsx! {
        div { class: "top-strip",
            div { class: "app-name", "MY BUDGET" }
            div { class: "user-area",
                span { "{user.username}" }
                button { class: "btn-text", onclick: move |_| on_logout.call(()), "LOGOUT" }
            }
        }
    }
}

#[component]
pub fn SectionSwitcher(current: Section, on_change: EventHandler<Section>) -> Element {
    rsx! {
        div { class: "section-switcher",
            for section in Section::all() {
                button {
                    class: if *section == current { "active" } else { "" },
                    onclick: move |_| on_change.call(*section),
                    "{section.label()}"
                }
            }
        }
    }
}

#[component]
pub fn Shell(
    user: PublicUser,
    current_section: Section,
    on_section_change: EventHandler<Section>,
    on_logout: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        div { id: "main",
            TopStrip { user: user, on_logout: on_logout }
            SectionSwitcher { current: current_section, on_change: on_section_change }
            div { class: "content container",
                {children}
            }
        }
    }
}
