use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;
use crate::components::{Inspector, SourceRail, Ticker, TopBar};
use crate::net::start_websocket;
use crate::state::{AppState, NavTab};
use crate::views::{AboutView, EndpointsView, LiveView, SystemView};

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state.clone());

    // Démarre la connexion WebSocket temps réel
    start_websocket(state.clone());

    let active_tab = state.active_tab;
    let event_store = state.event_store.clone();

    // Raccourcis clavier globaux
    let on_window_keydown = move |ev: KeyboardEvent| {
        // Ignorer si l'utilisateur est en train d'écrire dans un champ de texte
        if let Some(target) = ev.target() {
            if let Ok(elem) = target.dyn_into::<web_sys::HtmlElement>() {
                let tag = elem.tag_name().to_lowercase();
                if tag == "input" || tag == "textarea" {
                    return;
                }
            }
        }

        match ev.key().as_str() {
            "1" => active_tab.set(NavTab::Live),
            "2" => active_tab.set(NavTab::Endpoints),
            "3" => active_tab.set(NavTab::System),
            "4" => active_tab.set(NavTab::About),
            " " => {
                ev.prevent_default();
                event_store.toggle_pause();
            }
            _ => {}
        }
    };

    view! {
        <div class="app-shell" on:keydown=on_window_keydown tabindex="-1">
            <TopBar />

            <div class="app-main">
                <SourceRail />

                <div class="center-viewport">
                    {move || match active_tab.get() {
                        NavTab::Live => view! { <LiveView /> }.into_any(),
                        NavTab::Endpoints => view! { <EndpointsView /> }.into_any(),
                        NavTab::System => view! { <SystemView /> }.into_any(),
                        NavTab::About => view! { <AboutView /> }.into_any(),
                    }}
                </div>

                <Inspector />
            </div>

            <Ticker />
        </div>
    }
}
