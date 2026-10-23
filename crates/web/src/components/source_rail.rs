use std::collections::{BTreeMap, HashMap};
use leptos::prelude::*;
use hypatia_shared::SourceId;
use crate::state::AppState;

#[component]
pub fn SourceRail() -> impl IntoView {
    let state = expect_context::<AppState>();
    let store = state.event_store.clone();
    let rail_filter = RwSignal::new(String::new());

    // Calcul de l'arbre Sources -> Namespaces
    let tree_data = move || {
        let _ = store.version.get();
        let q = rail_filter.get().to_lowercase();

        let sources = [
            SourceId::LeagueClient,
            SourceId::RiotClient,
            SourceId::LiveGame,
            SourceId::System,
        ];

        let mut sources_map: HashMap<SourceId, BTreeMap<String, usize>> = HashMap::new();
        for src in sources {
            sources_map.insert(src, BTreeMap::new());
        }

        if let Ok(deque) = store.events.read() {
            for ev in deque.iter() {
                // Extrait le premier segment de l'URI comme namespace (ex: /lol-champ-select/... -> lol-champ-select)
                let trimmed = ev.uri.trim_start_matches('/');
                let ns = trimmed.split('/').next().unwrap_or("root").to_string();

                if !q.is_empty() && !ns.to_lowercase().contains(&q) {
                    continue;
                }

                *sources_map.entry(ev.source).or_default().entry(ns).or_default() += 1;
            }
        }

        // Renvoie dans l'ordre fixe des sources
        sources
            .into_iter()
            .map(|src| {
                let ns_map = sources_map.remove(&src).unwrap_or_default();
                (src, ns_map)
            })
            .collect::<Vec<_>>()
    };

    let on_ns_click = move |ns: String| {
        let current = store.filter_query.get_untracked();
        if current == ns {
            store.filter_query.set(String::new());
        } else {
            store.filter_query.set(ns);
        }
    };

    view! {
        <aside class="source-rail" aria-label="Explorateur de sources et namespaces">
            <div class="rail-header">
                <div class="rail-title-row">
                    <span class="rail-title">"SOURCES"</span>
                    <span class="kbd-badge">"/"</span>
                </div>
                <input
                    id="rail-search-input"
                    type="text"
                    class="search-input-box"
                    placeholder="Filtrer namespaces... (/)"
                    prop:value=move || rail_filter.get()
                    on:input=move |ev| rail_filter.set(event_target_value(&ev))
                />
            </div>

            <div class="rail-tree" role="tree">
                {move || {
                    let list = tree_data();
                    let current_filter = store.filter_query.get();

                    view! {
                        <div>
                            {list.into_iter().map(|(src_id, namespaces)| {
                                let (src_name, _src_cls) = match src_id {
                                    SourceId::RiotClient => ("Riot Client", "src-riot"),
                                    SourceId::LeagueClient => ("League Client", "src-lcu"),
                                    SourceId::LiveGame => ("Partie", "src-live"),
                                    SourceId::System => ("Système", "src-system"),
                                };

                                let total_evts: usize = namespaces.values().sum();

                                view! {
                                    <div class="tree-source-group" role="treeitem">
                                        <div class="tree-source-header">
                                            <span>{src_name}</span>
                                            <span class="tree-source-count tabular-nums">{total_evts}</span>
                                        </div>

                                        <div class="tree-namespaces-list">
                                            {namespaces.into_iter().map(|(ns, count)| {
                                                let ns_label = ns.clone();
                                                let ns_title = format!("Filtrer sur {}", ns);
                                                let is_active = current_filter == ns;
                                                let click_ns = ns.clone();

                                                view! {
                                                    <div
                                                        class=format!("tree-ns-item {}", if is_active { "active" } else { "" })
                                                        role="treeitem"
                                                        on:click=move |_| on_ns_click(click_ns.clone())
                                                        title=ns_title
                                                    >
                                                        <div class="ns-left">
                                                            <span class="activity-dot"></span>
                                                            <span>{ns_label}</span>
                                                        </div>
                                                        <span class="tree-ns-count tabular-nums">{count}</span>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }
                }}
            </div>
        </aside>
    }
}
