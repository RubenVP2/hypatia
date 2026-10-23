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

    // Cache local en mémoire des payloads complets (id -> StoredEvent)
    let payload_cache = Rc::new(RefCell::new(HashMap::<u64, StoredEvent>::new()));

    // État du chargement du payload courant
    let current_stored = RwSignal::new(Option::<StoredEvent>::None);
    let is_loading_payload = RwSignal::new(false);
    let fetch_error = RwSignal::new(Option::<String>::None);

    // Feedback de copie
    let copy_json_feedback = RwSignal::new(false);
    let copy_path_feedback = RwSignal::new(false);

    // Effet réactif lorsqu'un nouvel événement est sélectionné
    let cache_ref = payload_cache.clone();
    Effect::new(move |_| {
        let opt_id = selected_id_sig.get();
        match opt_id {
            Some(id) => {
                // Vérifier d'abord le cache
                if let Some(cached) = cache_ref.borrow().get(&id).cloned() {
                    current_stored.set(Some(cached));
                    is_loading_payload.set(false);
                    fetch_error.set(None);
                    return;
                }

                // Sinon lancer la requête réseau GET /api/events/{id}
                is_loading_payload.set(true);
                fetch_error.set(None);

                let c_ref = cache_ref.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_event_detail(id).await {
                        Ok(stored) => {
                            c_ref.borrow_mut().insert(id, stored.clone());
                            // Vérifier si l'ID est toujours celui actuellement sélectionné
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
        <aside class="inspector-pane" role="region" aria-label="Inspecteur de données et détails">
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
                                    <span class="inspector-title">"INSPECTEUR D'ÉVÉNEMENT"</span>
                                    <div class="inspector-actions">
                                        <button
                                            type="button"
                                            class="inspector-action-btn"
                                            on:click=on_copy_json
                                            title="Copier le payload JSON complet dans le presse-papiers"
                                        >
                                            {if copied_json { "Copié !" } else { "Copier JSON" }}
                                        </button>
                                        <button
                                            type="button"
                                            class="inspector-action-btn"
                                            on:click=on_copy_path
                                            title="Copier l'URI de l'endpoint"
                                        >
                                            {if copied_path { "Copié !" } else { "Copier chemin" }}
                                        </button>
                                        <button
                                            type="button"
                                            class="inspector-action-btn open-ep-btn"
                                            on:click=on_open_endpoint
                                            title="Basculer vers la vue Endpoints sur cette URI"
                                        >
                                            "Ouvrir l'endpoint ↗"
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
                                    <span class="inspector-meta-item">{format!("{} octets", ev.size)}</span>
                                    {if ev.has_secrets {
                                        view! {
                                            <span class="inspector-secret-badge">"🔒 Secrets masqués"</span>
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
                                        "Données"
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
                                        "Brut"
                                    </button>
                                </div>
                            </div>

                            <div class="inspector-body">
                                {move || {
                                    if is_loading_payload.get() {
                                        return view! {
                                            <div class="inspector-loading">
                                                <span class="spinner-dot"></span>
                                                <span>"Chargement du payload complet..."</span>
                                            </div>
                                        }.into_any();
                                    }

                                    if let Some(err) = fetch_error.get() {
                                        return view! {
                                            <div class="inspector-error">
                                                <span>"Erreur lors du chargement : " {err}</span>
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
                                                                    {if copied_raw { "Copié !" } else { "Copier tout le JSON" }}
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
                                            // Aperçu de secours si pas encore reçu
                                            view! {
                                                <pre class="inspector-payload-preview">
                                                    {if ev.preview.is_empty() {
                                                        "(Payload vide ou non capturé)".to_string()
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
                                    "Sélectionnez un événement dans le tableau ou dans le bandeau inférieur pour inspecter ses données."
                                </div>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </aside>
    }
}
