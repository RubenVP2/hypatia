use leptos::prelude::*;
use hypatia_shared::{EndpointOrigin, SourceId};
use crate::components::json_tree::JsonTree;
use crate::net::{call_endpoint, fetch_catalog};
use crate::state::AppState;

#[component]
pub fn EndpointsView() -> impl IntoView {
    let state = expect_context::<AppState>();
    let ep_store = state.endpoint_store.clone();
    let endpoints = ep_store.endpoints;
    let selected_endpoint = ep_store.selected_endpoint;
    let filter_query = ep_store.filter_query;
    let filter_source = ep_store.filter_source;
    let is_loading = ep_store.is_loading;
    let call_loading = ep_store.call_loading;
    let call_result = ep_store.call_result;

    // Load catalog on mount if list is empty
    Effect::new({
        let ep_store = ep_store.clone();
        move |_| {
            if ep_store.endpoints.get_untracked().is_empty() && !ep_store.is_loading.get_untracked() {
                ep_store.is_loading.set(true);
                let store_clone = ep_store.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_catalog(None).await {
                        Ok(list) => {
                            store_clone.set_endpoints(list);
                        }
                        Err(e) => {
                            store_clone.is_loading.set(false);
                            web_sys::console::warn_1(&format!("Catalog error: {e}").into());
                        }
                    }
                });
            }
        }
    });

    // Filtered endpoint list
    let filtered_endpoints = move || {
        let list = endpoints.get();
        let q = filter_query.get().to_lowercase().trim().to_string();
        let src = filter_source.get();

        list.into_iter()
            .filter(|ep| {
                if let Some(s) = src {
                    if ep.source != s {
                        return false;
                    }
                }
                if !q.is_empty() {
                    let path_match = ep.path_template.to_lowercase().contains(&q);
                    let ns_match = ep.namespace.to_lowercase().contains(&q);
                    let desc_match = ep
                        .description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&q))
                        .unwrap_or(false);
                    return path_match || ns_match || desc_match;
                }
                true
            })
            .collect::<Vec<_>>()
    };

    // Handler to call endpoint via GET
    let on_call_endpoint = {
        let ep_store = ep_store.clone();
        move |_| {
            if let Some(ep) = ep_store.selected_endpoint.get_untracked() {
                if !ep.is_callable_get() || ep_store.call_loading.get_untracked() {
                    return;
                }

                ep_store.call_loading.set(true);
                ep_store.call_result.set(None);

                let store_clone = ep_store.clone();
                let path = ep.path_template.clone();
                let src = ep.source;

                wasm_bindgen_futures::spawn_local(async move {
                    let res = call_endpoint(src, &path).await;
                    store_clone.call_result.set(Some(res));
                    store_clone.call_loading.set(false);
                });
            }
        }
    };

    // Recent events for selected endpoint
    let recent_events_for_selected = {
        let state = state.clone();
        move || {
            let opt_ep = selected_endpoint.get();
            let _ = state.event_store.version.get();

            match opt_ep {
                Some(ep) => {
                    if let Ok(deque) = state.event_store.events.read() {
                        deque
                            .iter()
                            .rev()
                            .filter(|e| e.uri == ep.path_template)
                            .take(15)
                            .cloned()
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                }
                None => Vec::new(),
            }
        }
    };

    view! {
        <main class="endpoints-view-layout" role="main" aria-label="Riot endpoint explorer">
            <section class="endpoints-list-pane">
                <div class="endpoints-list-header">
                    <div class="endpoints-title-row">
                        <h2 class="endpoints-pane-title">"ENDPOINT CATALOG"</h2>
                        <span class="endpoints-count tabular-nums">
                            {move || format!("{} found", filtered_endpoints().len())}
                        </span>
                    </div>

                    <input
                        type="text"
                        class="search-input-box"
                        placeholder="Search by URI, namespace, or description..."
                        prop:value=move || filter_query.get()
                        on:input=move |ev| filter_query.set(event_target_value(&ev))
                    />

                    <div class="endpoints-source-filters">
                        <button
                            type="button"
                            class=format!("filter-chip {}", if filter_source.get().is_none() { "active" } else { "" })
                            on:click=move |_| filter_source.set(None)
                        >
                            "All"
                        </button>
                        <button
                            type="button"
                            class=format!("filter-chip {}", if filter_source.get() == Some(SourceId::LeagueClient) { "active" } else { "" })
                            on:click=move |_| filter_source.set(Some(SourceId::LeagueClient))
                        >
                            "LeagueClient"
                        </button>
                        <button
                            type="button"
                            class=format!("filter-chip {}", if filter_source.get() == Some(SourceId::RiotClient) { "active" } else { "" })
                            on:click=move |_| filter_source.set(Some(SourceId::RiotClient))
                        >
                            "RiotClient"
                        </button>
                        <button
                            type="button"
                            class=format!("filter-chip {}", if filter_source.get() == Some(SourceId::LiveGame) { "active" } else { "" })
                            on:click=move |_| filter_source.set(Some(SourceId::LiveGame))
                        >
                            "Game :2999"
                        </button>
                    </div>
                </div>

                <div class="endpoints-scroll-list" role="listbox">
                    {move || {
                        if is_loading.get() {
                            return view! {
                                <div class="endpoints-loading">
                                    <span class="spinner-dot"></span>
                                    <span>"Loading catalog (/help & OpenAPI)..."</span>
                                </div>
                            }.into_any();
                        }

                        let list = filtered_endpoints();
                        if list.is_empty() {
                            return view! {
                                <div class="endpoints-empty">
                                    "No endpoint matches the search criteria."
                                </div>
                            }.into_any();
                        }

                        let cur_sel = selected_endpoint.get();
                        let store_for_click = ep_store.clone();

                        list.into_iter().map(|ep| {
                            let is_selected = cur_sel.as_ref().map(|s| s.path_template == ep.path_template).unwrap_or(false);
                            let ep_clone = ep.clone();
                            let ep_for_click = ep.clone();
                            let store_c = store_for_click.clone();

                            let origin_tag = match ep.origin {
                                EndpointOrigin::Declared => "Declared",
                                EndpointOrigin::Observed => "Observed",
                                EndpointOrigin::Both => "Both",
                            };

                            let (src_short, src_cls) = match ep.source {
                                SourceId::RiotClient => ("RC", "tag-rc"),
                                SourceId::LeagueClient => ("LCU", "tag-lcu"),
                                SourceId::LiveGame => ("GAME", "tag-jeu"),
                                SourceId::System => ("SYS", "tag-sys"),
                            };

                            view! {
                                <div
                                    class=format!("endpoint-list-item {}", if is_selected { "selected" } else { "" })
                                    role="option"
                                    aria-selected=is_selected.to_string()
                                    on:click=move |_| store_c.select_endpoint(ep_for_click.clone())
                                >
                                    <div class="ep-item-top">
                                        <span class=format!("src-tag {}", src_cls)>{src_short}</span>
                                        <span class="ep-method-badge">{ep_clone.method}</span>
                                        <span class="ep-path tabular-nums" title=ep_clone.path_template.clone()>
                                            {ep_clone.path_template.clone()}
                                        </span>
                                    </div>
                                    <div class="ep-item-bottom">
                                        <span class="ep-namespace">{ep_clone.namespace}</span>
                                        <span class="ep-origin-badge">{origin_tag}</span>
                                        {if ep_clone.event_count > 0 {
                                            view! {
                                                <span class="ep-events-count tabular-nums">
                                                    {format!("{} evt", ep_clone.event_count)}
                                                </span>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }}
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>().into_any()
                    }}
                </div>
            </section>

            <section class="endpoint-detail-pane">
                {move || {
                    match selected_endpoint.get() {
                        Some(ep) => {
                            let path = ep.path_template.clone();
                            let is_callable = ep.is_callable_get();
                            let loading_call = call_loading.get();
                            let call_res = call_result.get();
                            let recent_evts = recent_events_for_selected();

                            let origin_tag = match ep.origin {
                                EndpointOrigin::Declared => "Declared (/help or OpenAPI)",
                                EndpointOrigin::Observed => "Observed in WAMP traffic",
                                EndpointOrigin::Both => "Declared & Observed",
                            };

                            let (src_name, src_cls) = match ep.source {
                                SourceId::RiotClient => ("Riot Client", "tag-rc"),
                                SourceId::LeagueClient => ("League Client", "tag-lcu"),
                                SourceId::LiveGame => ("Live Game (port 2999)", "tag-jeu"),
                                SourceId::System => ("System", "tag-sys"),
                            };

                            let on_call_fn = on_call_endpoint.clone();

                            view! {
                                <div class="endpoint-detail-content">
                                    <header class="endpoint-detail-header">
                                        <div class="ep-detail-tags">
                                            <span class=format!("src-tag {}", src_cls)>{src_name}</span>
                                            <span class="ep-method-badge">{ep.method.clone()}</span>
                                            <span class="ep-origin-pill">{origin_tag}</span>
                                        </div>

                                        <h1 class="ep-detail-path tabular-nums">{path.clone()}</h1>

                                        {if let Some(desc) = &ep.description {
                                            view! {
                                                <p class="ep-detail-desc">{desc.clone()}</p>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }}

                                        <div class="ep-meta-stats tabular-nums">
                                            <span>"Namespace: " <strong>{ep.namespace}</strong></span>
                                            <span class="meta-sep">"•"</span>
                                            <span>"Observed events: " <strong>{ep.event_count}</strong></span>
                                            <span class="meta-sep">"•"</span>
                                            <span>"Volume: " <strong>{format!("{} bytes", ep.bytes_total)}</strong></span>
                                        </div>

                                        <div class="ep-call-control">
                                            {if is_callable {
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="call-get-btn"
                                                        disabled=loading_call
                                                        on:click=move |ev| on_call_fn(ev)
                                                    >
                                                        {if loading_call {
                                                            "Calling..."
                                                        } else {
                                                            "Call (GET)"
                                                        }}
                                                    </button>
                                                }.into_any()
                                            } else {
                                                let reason = if !ep.method.eq_ignore_ascii_case("GET") {
                                                    format!("Method {} not supported directly", ep.method)
                                                } else {
                                                    "Requires path parameters {param}".to_string()
                                                };
                                                view! {
                                                    <div class="call-disabled-box" title=reason.clone()>
                                                        <button type="button" class="call-get-btn disabled" disabled=true>
                                                            "Call (GET)"
                                                        </button>
                                                        <span class="call-disabled-reason">
                                                            {format!("(Disabled: {})", reason)}
                                                        </span>
                                                    </div>
                                                }.into_any()
                                            }}
                                        </div>
                                    </header>

                                    {if let Some(res) = call_res {
                                        view! {
                                            <section class="ep-call-result-section">
                                                <div class="ep-result-header">
                                                    <span class="ep-result-title">"GET CALL RESULT"</span>
                                                    {match &res {
                                                        Ok(call_resp) => {
                                                            view! {
                                                                <span class=format!("http-status-badge status-{}", call_resp.status / 100)>
                                                                    {format!("HTTP {}", call_resp.status)}
                                                                </span>
                                                            }.into_any()
                                                        }
                                                        Err(_) => {
                                                            view! {
                                                                <span class="http-status-badge status-err">
                                                                    "ERROR"
                                                                </span>
                                                            }.into_any()
                                                        }
                                                    }}
                                                </div>

                                                <div class="ep-result-body">
                                                    {match res {
                                                        Ok(call_resp) => {
                                                            view! {
                                                                <JsonTree value=call_resp.data />
                                                            }.into_any()
                                                        }
                                                        Err(e) => {
                                                            view! {
                                                                <div class="ep-call-error">{e}</div>
                                                            }.into_any()
                                                        }
                                                    }}
                                                </div>
                                            </section>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }}

                                    <section class="ep-recent-events-section">
                                        <h3 class="ep-recent-title">
                                            {format!("Recent events ({})", recent_evts.len())}
                                        </h3>
                                        {if recent_evts.is_empty() {
                                            view! {
                                                <div class="ep-recent-empty">
                                                    "No WAMP events captured recently on this URI."
                                                </div>
                                            }.into_any()
                                        } else {
                                            let st = state.clone();
                                            view! {
                                                <div class="ep-recent-list">
                                                    {recent_evts.into_iter().map(|ev| {
                                                        let ev_id = ev.id;
                                                        let st_c = st.clone();
                                                        view! {
                                                            <div
                                                                class="ep-recent-row"
                                                                on:click=move |_| st_c.event_store.select_event(ev_id)
                                                            >
                                                                <span class="tabular-nums">{format!("#{}", ev.id)}</span>
                                                                <span class="meta-sep">"•"</span>
                                                                <span>{format!("{:?}", ev.kind)}</span>
                                                                <span class="meta-sep">"•"</span>
                                                                <span class="tabular-nums">{format!("{} bytes", ev.size)}</span>
                                                                <span class="ep-preview-text">{ev.preview}</span>
                                                            </div>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            }.into_any()
                                        }}
                                    </section>
                                </div>
                            }.into_any()
                        }
                        None => {
                            view! {
                                <div class="endpoint-empty-selection">
                                    <div class="empty-state-icon">"⊞"</div>
                                    <h2 class="empty-state-heading">"Select an endpoint"</h2>
                                    <p class="empty-state-text">
                                        "Select an API from the catalog on the left to view its documentation, inspect observed data, and issue a direct read-only call (GET)."
                                    </p>
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </section>
        </main>
    }
}
