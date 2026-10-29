use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, HtmlInputElement, KeyboardEvent};
use crate::components::{CommandPalette, Inspector, SourceRail, Ticker, TopBar, WsDisconnectedBanner};
use crate::net::start_websocket;
use crate::state::{AppState, NavTab};
use crate::views::{AboutView, EndpointsView, LiveView, RecapView, SystemView};

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state.clone());

    // Starts real-time WebSocket connection
    start_websocket(state.clone());

    let active_tab = state.active_tab;
    let event_store = state.event_store.clone();
    let is_connected = state.is_connected;
    let command_palette_open = state.command_palette_open;
    let left_rail_width = state.left_rail_width;
    let inspector_width = state.inspector_width;

    // Global keyboard shortcuts
    let on_window_keydown = move |ev: KeyboardEvent| {
        let key = ev.key();

        // Ctrl+K / Cmd+K shortcut accessible from anywhere
        if (ev.ctrl_key() || ev.meta_key()) && (key == "k" || key == "K") {
            ev.prevent_default();
            command_palette_open.update(|v| *v = !*v);
            return;
        }

        // Ignore single-key shortcuts if the user is in a text input
        if let Some(target) = ev.target() {
            if let Ok(elem) = target.dyn_into::<HtmlElement>() {
                let tag = elem.tag_name().to_lowercase();
                if tag == "input" || tag == "textarea" {
                    return;
                }
            }
        }

        match key.as_str() {
            "1" => active_tab.set(NavTab::Live),
            "2" => active_tab.set(NavTab::Endpoints),
            "3" => active_tab.set(NavTab::System),
            "4" => active_tab.set(NavTab::About),
            "5" | "r" | "R" => active_tab.set(NavTab::Recap),
            " " => {
                ev.prevent_default();
                event_store.toggle_pause();
            }
            "/" => {
                ev.prevent_default();
                if let Some(window) = web_sys::window() {
                    if let Some(doc) = window.document() {
                        if let Some(elem) = doc.get_element_by_id("rail-search-input") {
                            if let Ok(input) = elem.dyn_into::<HtmlInputElement>() {
                                let _ = input.focus();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    };

    // Keyboard resizing for the left rail
    let on_rail_resizer_keydown = move |ev: KeyboardEvent| {
        match ev.key().as_str() {
            "ArrowLeft" => {
                ev.prevent_default();
                left_rail_width.update(|w| *w = (*w - 16).max(200));
            }
            "ArrowRight" => {
                ev.prevent_default();
                left_rail_width.update(|w| *w = (*w + 16).min(400));
            }
            _ => {}
        }
    };

    // Keyboard resizing for the inspector
    let on_inspector_resizer_keydown = move |ev: KeyboardEvent| {
        match ev.key().as_str() {
            "ArrowLeft" => {
                ev.prevent_default();
                inspector_width.update(|w| *w = (*w + 16).min(650));
            }
            "ArrowRight" => {
                ev.prevent_default();
                inspector_width.update(|w| *w = (*w - 16).max(320));
            }
            _ => {}
        }
    };

    view! {
        <div class="app-shell" on:keydown=on_window_keydown tabindex="-1">
            <TopBar />

            {move || {
                if !is_connected.get() {
                    view! { <WsDisconnectedBanner /> }.into_any()
                } else if state.is_demo.get() {
                    view! {
                        <div class="demo-banner" role="status">
                            <span class="demo-badge">"DEMO"</span>
                            <span>"Demonstration Mode (synthetic data)"</span>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}

            <div class="app-main">
                <div
                    class="rail-container"
                    style=move || format!("width: {}px; min-width: 200px;", left_rail_width.get())
                >
                    <SourceRail />
                </div>

                <div
                    class="panel-resizer"
                    role="separator"
                    tabindex="0"
                    aria-orientation="vertical"
                    aria-label="Resize left rail (Left/Right Arrows)"
                    on:keydown=on_rail_resizer_keydown
                ></div>

                <div class="center-viewport">
                    {move || match active_tab.get() {
                        NavTab::Live => view! { <LiveView /> }.into_any(),
                        NavTab::Recap => view! { <RecapView /> }.into_any(),
                        NavTab::Endpoints => view! { <EndpointsView /> }.into_any(),
                        NavTab::System => view! { <SystemView /> }.into_any(),
                        NavTab::About => view! { <AboutView /> }.into_any(),
                    }}
                </div>

                <div
                    class="panel-resizer"
                    role="separator"
                    tabindex="0"
                    aria-orientation="vertical"
                    aria-label="Resize inspector (Left/Right Arrows)"
                    on:keydown=on_inspector_resizer_keydown
                ></div>

                <div
                    class="inspector-container"
                    style=move || format!("width: {}px; min-width: 320px;", inspector_width.get())
                >
                    <Inspector />
                </div>
            </div>

            <Ticker />

            <CommandPalette />
        </div>
    }
}
