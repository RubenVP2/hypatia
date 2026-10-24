use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, KeyboardEvent};
use hypatia_shared::EndpointInfo;
use crate::state::{AppState, NavTab};

#[derive(Clone, Debug)]
pub enum CommandAction {
    Nav(NavTab),
    TogglePause,
    ClearEvents,
    ToggleSecrets,
    OpenEndpoint(EndpointInfo),
}

#[derive(Clone, Debug)]
pub struct PaletteItem {
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub action: CommandAction,
}

#[component]
pub fn CommandPalette() -> impl IntoView {
    let state = expect_context::<AppState>();
    let is_open = state.command_palette_open;
    let query = RwSignal::new(String::new());
    let selected_index = RwSignal::new(0usize);

    let endpoints = state.endpoint_store.endpoints;
    let is_paused = state.event_store.is_paused;

    // Compute the full list of commands
    let filtered_items = move || {
        let q = query.get().to_lowercase().trim().to_string();
        let paused = is_paused.get();

        let mut items = Vec::new();

        // 1. Navigation
        items.push(PaletteItem {
            title: "Live View".to_string(),
            subtitle: "Event stream view, phase, and timer".to_string(),
            category: "Navigation".to_string(),
            action: CommandAction::Nav(NavTab::Live),
        });
        items.push(PaletteItem {
            title: "Endpoints View".to_string(),
            subtitle: "API catalog, details, and GET calls".to_string(),
            category: "Navigation".to_string(),
            action: CommandAction::Nav(NavTab::Endpoints),
        });
        items.push(PaletteItem {
            title: "System View".to_string(),
            subtitle: "Riot processes, TCP sockets, and connections".to_string(),
            category: "Navigation".to_string(),
            action: CommandAction::Nav(NavTab::System),
        });
        items.push(PaletteItem {
            title: "About View".to_string(),
            subtitle: "Read-only guarantees, help, and Replay setup".to_string(),
            category: "Navigation".to_string(),
            action: CommandAction::Nav(NavTab::About),
        });

        // 2. Stream actions
        items.push(PaletteItem {
            title: if paused { "Resume event stream" } else { "Pause event stream" }.to_string(),
            subtitle: if paused { "Resume continuous event reception" } else { "Freeze live event stream" }.to_string(),
            category: "Action".to_string(),
            action: CommandAction::TogglePause,
        });
        items.push(PaletteItem {
            title: "Clear event stream".to_string(),
            subtitle: "Clear all local events from memory".to_string(),
            category: "Action".to_string(),
            action: CommandAction::ClearEvents,
        });
        items.push(PaletteItem {
            title: "Toggle secrets visibility".to_string(),
            subtitle: "Mask or reveal tokens and passwords across the dashboard".to_string(),
            category: "Action".to_string(),
            action: CommandAction::ToggleSecrets,
        });

        // 3. Catalog endpoints
        let ep_list = endpoints.get();
        for ep in ep_list {
            items.push(PaletteItem {
                title: ep.path_template.clone(),
                subtitle: format!("{:?} • {}", ep.source, ep.description.as_deref().unwrap_or("API Endpoint")),
                category: "Endpoint".to_string(),
                action: CommandAction::OpenEndpoint(ep),
            });
        }

        // Substring filtering
        if q.is_empty() {
            items.truncate(15);
            items
        } else {
            items
                .into_iter()
                .filter(|it| {
                    it.title.to_lowercase().contains(&q)
                        || it.subtitle.to_lowercase().contains(&q)
                        || it.category.to_lowercase().contains(&q)
                })
                .take(30)
                .collect()
        }
    };

    let execute_item = {
        let state = state.clone();
        move |item: PaletteItem| {
            match item.action {
                CommandAction::Nav(tab) => {
                    state.active_tab.set(tab);
                }
                CommandAction::TogglePause => {
                    state.event_store.toggle_pause();
                }
                CommandAction::ClearEvents => {
                    state.event_store.clear();
                }
                CommandAction::ToggleSecrets => {
                    state.secrets_hidden.update(|v| *v = !*v);
                }
                CommandAction::OpenEndpoint(ep) => {
                    state.endpoint_store.select_endpoint(ep);
                    state.active_tab.set(NavTab::Endpoints);
                }
            }
            is_open.set(false);
            query.set(String::new());
        }
    };

    let on_keydown = {
        let execute = execute_item.clone();
        move |ev: KeyboardEvent| {
            let key = ev.key();
            let items = filtered_items();
            let count = items.len();

            if count == 0 {
                if key == "Escape" {
                    is_open.set(false);
                }
                return;
            }

            match key.as_str() {
                "ArrowDown" => {
                    ev.prevent_default();
                    selected_index.update(|idx| {
                        *idx = (*idx + 1) % count;
                    });
                }
                "ArrowUp" => {
                    ev.prevent_default();
                    selected_index.update(|idx| {
                        *idx = if *idx == 0 { count - 1 } else { *idx - 1 };
                    });
                }
                "Enter" => {
                    ev.prevent_default();
                    let cur = selected_index.get_untracked();
                    if let Some(item) = items.get(cur) {
                        execute(item.clone());
                    }
                }
                "Escape" => {
                    ev.prevent_default();
                    is_open.set(false);
                }
                _ => {}
            }
        }
    };

    // Auto-focus input when palette opens
    Effect::new(move |_| {
        if is_open.get() {
            selected_index.set(0);
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    let _ = gloo_timers::future::TimeoutFuture::new(50);
                    if let Some(elem) = document.get_element_by_id("palette-search-input") {
                        if let Ok(input) = elem.dyn_into::<HtmlInputElement>() {
                            let _ = input.focus();
                        }
                    }
                }
            }
        }
    });

    view! {
        {move || {
            if !is_open.get() {
                return ().into_any();
            }

            let items = filtered_items();
            let current_sel = selected_index.get();
            let exec_click = execute_item.clone();

            view! {
                <div
                    class="command-palette-backdrop"
                    on:click=move |_| is_open.set(false)
                    role="presentation"
                >
                    <div
                        class="command-palette-modal"
                        on:click=move |ev| ev.stop_propagation()
                        on:keydown=on_keydown.clone()
                        role="dialog"
                        aria-modal="true"
                        aria-label="Command palette"
                    >
                        <div class="palette-search-header">
                            <span class="palette-search-icon">"⌘"</span>
                            <input
                                id="palette-search-input"
                                type="text"
                                class="palette-input"
                                placeholder="Search views, commands, or endpoints (↑/↓ to navigate, Enter)..."
                                prop:value=move || query.get()
                                on:input=move |ev| {
                                    query.set(event_target_value(&ev));
                                    selected_index.set(0);
                                }
                            />
                            <span class="kbd-badge" on:click=move |_| is_open.set(false)>"Esc"</span>
                        </div>

                        <div class="palette-results-list" role="listbox">
                            {if items.is_empty() {
                                view! {
                                    <div class="palette-empty">"No results found."</div>
                                }.into_any()
                            } else {
                                items.into_iter().enumerate().map(|(idx, item)| {
                                    let is_selected = idx == current_sel;
                                    let click_item = item.clone();
                                    let exec_fn = exec_click.clone();

                                    view! {
                                        <div
                                            class=format!("palette-item {}", if is_selected { "selected" } else { "" })
                                            role="option"
                                            aria-selected=is_selected.to_string()
                                            on:click=move |_| exec_fn(click_item.clone())
                                        >
                                            <div class="palette-item-left">
                                                <span class="palette-item-title">{item.title}</span>
                                                <span class="palette-item-subtitle">{item.subtitle}</span>
                                            </div>
                                            <span class="palette-item-badge">{item.category}</span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>().into_any()
                            }}
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
