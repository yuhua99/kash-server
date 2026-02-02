use dioxus::prelude::*;

#[component]
pub fn Overlay(title: String, on_close: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div {
            class: "overlay",
            onclick: move |_| on_close.call(()),
            div {
                class: "overlay-content",
                onclick: move |e| e.stop_propagation(),
                div { class: "overlay-header",
                    h2 { "{title}" }
                    button { class: "btn-text", onclick: move |_| on_close.call(()), "CLOSE" }
                }
                {children}
            }
        }
    }
}
