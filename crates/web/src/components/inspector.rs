use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use hypatia_shared::StoredEvent;
use crate::components::json_diff::JsonDiff;
use crate::components::json_tree::JsonTree;
use crate::net::fetch_event_detail;
use crate::state::{AppState, InspectorTab, NavTab};

#[component]
pub fn Inspector() -> impl IntoView {
    let state = expect_context::<AppState>();
    let selected_event_sig = state.event_store.selected_event;
    let selected_id_sig = state.event_store.selected_id;
    let active_tab = state.inspector_tab;

    // In-memory local cache of full payloads (id -> StoredEvent)
    let payload_cache = Rc::new(RefCell::new(HashMap::<u64, StoredEvent>::new()));

    // Current payload loading state
    let current_stored = RwSignal::new(Option::<StoredEvent>::None);
    let is_loading_payload = RwSignal::new(false);
    let fetch_error = RwSignal::new(Option::<String>::None);

    // Copy feedback
    let copy_json_feedback = RwSignal::new(false);
    let copy_path_feedback = RwSignal::new(false);

    // Reactive effect when a new event is selected
    let cache_ref = payload_cache.clone();
    Effect::new(move |_| {
        let opt_id = selected_id_sig.get();
        match opt_id {
            Some(id) => {
                // First check the cache
                if let Some(cached) = cache_ref.borrow().get(&id).cloned() {
                    current_stored.set(Some(cached));
                    is_loading_payload.set(false);
                    fetch_error.set(None);
                    return;
                }

                // Otherwise issue network request GET /api/events/{id}
                is_loading_payload.set(true);
                fetch_error.set(None);

                let c_ref = cache_ref.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_event_detail(id).await {
                        Ok(stored) => {
                            c_ref.borrow_mut().insert(id, stored.clone());
                            // Check if ID is still the currently selected one
                            if selected_id_sig.get_untracked() == Some(id) {
                                current_stored.set(Some(stored));
                                is_loading_payload.set(false);
                            }
                        }
                        Err(err) => {
                            if selected_id_sig.get_untracked() == Some(id) {
                                is_loading_payload.set(false);
                                fetch_error.set(Some(err));
                            }
                        }
                    }
                });
            }
            None => {
                current_stored.set(None);
                is_loading_payload.set(false);
                fetch_error.set(None);
            }
        }
    });

    view! {
        <aside class="inspector-pane" role="region" aria-label="Data and details inspector">
            {move || {
                match selected_event_sig.get() {
                    Some(ev) => {
                        let uri_clone = ev.uri.clone();
                        let tab = active_tab.get();
                        let copied_json = copy_json_feedback.get();
                        let copied_path = copy_path_feedback.get();

                        let state_for_open = state.clone();
                        let uri_for_open = ev.uri.clone();
                        let on_open_endpoint = move |_| {
                            state_for_open.endpoint_store.select_by_uri(&uri_for_open);
                            state_for_open.active_tab.set(NavTab::Endpoints);
                        };

                        let on_copy_json = move |_| {
                            if let Some(stored) = current_stored.get_untracked() {
                                let json_str = serde_json::to_string_pretty(&stored.payload).unwrap_or_default();
                                if let Some(window) = web_sys::window() {
                                    let clipboard = window.navigator().clipboard();
                                    let _ = clipboard.write_text(&json_str);
                                    copy_json_feedback.set(true);

                                    wasm_bindgen_futures::spawn_local(async move {
                                        TimeoutFuture::new(2000).await;
                                        copy_json_feedback.set(false);
                                    });
                                }
                            }
                        };

                        let uri_for_copy = ev.uri.clone();
                        let on_copy_path = move |_| {
                            if let Some(window) = web_sys::window() {
                                let clipboard = window.navigator().clipboard();
                                let _ = clipboard.write_text(&uri_for_copy);
                                copy_path_feedback.set(true);

                                wasm_bindgen_futures::spawn_local(async move {
                                    TimeoutFuture::new(2000).await;
                                    copy_path_feedback.set(false);
                                });
                            }
                        };

                        view! {
                            <div class="inspector-header">
                                <div class="inspector-header-top">
                                    <span class="inspector-title">"EVENT INSPECTOR"</span>
                                    <div class="inspector-actions">
                                        <button
                                            type="button"
                                            class="inspector-action-btn"
                                            on:click=on_copy_json
                                            title="Copy full JSON payload to clipboard"
                                        >
                                            {if copied_json { "Copied!" } else { "Copy JSON" }}
                                        </button>
                                        <button
                                            type="button"
                                            class="inspector-action-btn"
                                            on:click=on_copy_path
                                            title="Copy endpoint URI"
                                        >
                                            {if copied_path { "Copied!" } else { "Copy path" }}
                                        </button>
                                        <button
                                            type="button"
                                            class="inspector-action-btn open-ep-btn"
                                            on:click=on_open_endpoint
                                            title="Switch to Endpoints view for this URI"
                                        >
                                            "Open endpoint ↗"
                                        </button>
                                    </div>
                                </div>

                                <div class="inspector-uri tabular-nums" title=uri_clone.clone()>
                                    {uri_clone.clone()}
                                </div>

                                <div class="inspector-meta-row tabular-nums">
                                    <span class="inspector-meta-item">{format!("ID #{}", ev.id)}</span>
                                    <span class="meta-sep">"•"</span>
                                    <span class="inspector-meta-item">{format!("{:?}", ev.source)}</span>
                                    <span class="meta-sep">"•"</span>
                                    <span class="inspector-meta-item">{format!("{:?}", ev.kind)}</span>
                                    <span class="meta-sep">"•"</span>
                                    <span class="inspector-meta-item">{format!("{} bytes", ev.size)}</span>
                                    {if ev.has_secrets {
                                        view! {
                                            <span class="inspector-secret-badge">"🔒 Secrets hidden"</span>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }}
                                </div>

                                <div class="inspector-tabs" role="tablist">
                                    <button
                                        type="button"
                                        class=format!("inspector-tab-btn {}", if tab == InspectorTab::Data { "active" } else { "" })
                                        role="tab"
                                        aria-selected=(tab == InspectorTab::Data).to_string()
                                        on:click=move |_| active_tab.set(InspectorTab::Data)
                                    >
                                        "JSON Tree"
                                    </button>
                                    <button
                                        type="button"
                                        class=format!("inspector-tab-btn {}", if tab == InspectorTab::Diff { "active" } else { "" })
                                        role="tab"
                                        aria-selected=(tab == InspectorTab::Diff).to_string()
                                        on:click=move |_| active_tab.set(InspectorTab::Diff)
                                    >
                                        "Diff"
                                    </button>
                                    <button
                                        type="button"
                                        class=format!("inspector-tab-btn {}", if tab == InspectorTab::Raw { "active" } else { "" })
                                        role="tab"
                                        aria-selected=(tab == InspectorTab::Raw).to_string()
                                        on:click=move |_| active_tab.set(InspectorTab::Raw)
                                    >
                                        "Raw"
                                    </button>
                                </div>
                            </div>

                            <div class="inspector-body">
                                {move || {
                                    if is_loading_payload.get() {
                                        return view! {
                                            <div class="inspector-loading">
                                                <span class="spinner-dot"></span>
                                                <span>"Loading full payload..."</span>
                                            </div>
                                        }.into_any();
                                    }

                                    if let Some(err) = fetch_error.get() {
                                        return view! {
                                            <div class="inspector-error">
                                                <span>"Error loading payload: " {err}</span>
                                            </div>
                                        }.into_any();
                                    }

                                    match current_stored.get() {
                                        Some(stored) => {
                                            match active_tab.get() {
                                                InspectorTab::Data => {
                                                    view! {
                                                        <JsonTree value=stored.payload.clone() />
                                                    }.into_any()
                                                }
                                                InspectorTab::Diff => {
                                                    view! {
                                                        <JsonDiff
                                                            current=stored.payload.clone()
                                                            previous=stored.previous_payload.clone()
                                                        />
                                                    }.into_any()
                                                }
                                                InspectorTab::Raw => {
                                                    let pretty_json = serde_json::to_string_pretty(&stored.payload)
                                                        .unwrap_or_else(|_| "{}".to_string());
                                                    let copied_raw = copy_json_feedback.get();

                                                    view! {
                                                        <div class="raw-json-container">
                                                            <div class="raw-json-toolbar">
                                                                <button
                                                                    type="button"
                                                                    class="raw-copy-btn"
                                                                    on:click=on_copy_json
                                                                >
                                                                    {if copied_raw { "Copied!" } else { "Copy full JSON" }}
                                                                </button>
                                                            </div>
                                                            <pre class="raw-json-pre">
                                                                <code>{pretty_json}</code>
                                                            </pre>
                                                        </div>
                                                    }.into_any()
                                                }
                                            }
                                        }
                                        None => {
                                            // Fallback preview if not yet received
                                            view! {
                                                <pre class="inspector-payload-preview">
                                                    {if ev.preview.is_empty() {
                                                        "(Payload empty or not captured)".to_string()
                                                    } else {
                                                        ev.preview.clone()
                                                    }}
                                                </pre>
                                            }.into_any()
                                        }
                                    }
                                }}
                            </div>
                        }.into_any()
                    }
                    None => {
                        view! {
                            <div class="empty-state" style="margin-top: 60px;">
                                <div class="empty-state-icon">"⌕"</div>
                                <div class="empty-state-text">
                                    "Select an event in the table or the bottom ticker to inspect its data."
                                </div>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </aside>
    }
}
