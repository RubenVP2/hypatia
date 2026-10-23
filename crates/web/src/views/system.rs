use leptos::prelude::*;
use hypatia_shared::SourceId;
use crate::net::fetch_system;
use crate::state::AppState;

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} Go", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} Mo", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} Ko", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

#[component]
pub fn SystemView() -> impl IntoView {
    let state = expect_context::<AppState>();
    let sys_store = state.system_store.clone();
    let snapshot_sig = sys_store.snapshot;
    let is_loading = sys_store.is_loading;
    let error_sig = sys_store.error;

    // Charger le snapshot système au montage si absent
    Effect::new({
        let sys_store = sys_store.clone();
        move |_| {
            if sys_store.snapshot.get_untracked().is_none() && !sys_store.is_loading.get_untracked() {
                sys_store.is_loading.set(true);
                let store_c = sys_store.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_system().await {
                        Ok(snap) => {
                            store_c.set_snapshot(snap);
                        }
                        Err(err) => {
                            store_c.is_loading.set(false);
                            store_c.error.set(Some(err));
                        }
                    }
                });
            }
        }
    });

    let refresh_system = {
        let sys_store = sys_store.clone();
        move |_| {
            sys_store.is_loading.set(true);
            let store_c = sys_store.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_system().await {
                    Ok(snap) => {
                        store_c.set_snapshot(snap);
                    }
                    Err(err) => {
                        store_c.is_loading.set(false);
                        store_c.error.set(Some(err));
                    }
                }
            });
        }
    };

    view! {
        <main class="system-view-container" role="main" aria-label="Vue Système et Processus Riot">
            <aside class="system-banner" role="note">
                <span class="system-banner-icon">"🔒"</span>
                <span class="system-banner-text">
                    "Le trafic vers les serveurs Riot est chiffré : on voit les connexions, pas leur contenu."
                </span>
                <button
                    type="button"
                    class="system-refresh-btn"
                    disabled=move || is_loading.get()
                    on:click=refresh_system
                >
                    {move || if is_loading.get() { "Actualisation..." } else { "Actualiser" }}
                </button>
            </aside>

            {move || {
                if let Some(err) = error_sig.get() {
                    return view! {
                        <div class="system-error-banner">
                            <span>"Erreur lors de la récupération de l'état système : " {err}</span>
                        </div>
                    }.into_any();
                }

                match snapshot_sig.get() {
                    Some(snap) => {
                        let proc_count = snap.processes.len();
                        let sock_count = snap.sockets.len();
                        let conn_count = snap.connections.len();

                        let mut pid_map = std::collections::HashMap::new();
                        for p in &snap.processes {
                            pid_map.insert(p.pid, (p.name.clone(), p.source));
                        }

                        view! {
                            <div class="system-tables-grid">
                                <section class="system-card" aria-labelledby="proc-table-title">
                                    <header class="system-card-header">
                                        <div class="card-header-left">
                                            <h2 id="proc-table-title" class="system-card-title">"PROCESSUS RIOT"</h2>
                                            <span class="card-badge tabular-nums">{proc_count}</span>
                                        </div>
                                    </header>

                                    <div class="system-table-wrap">
                                        <table class="system-data-table" role="table">
                                            <thead>
                                                <tr>
                                                    <th>"Nom"</th>
                                                    <th>"PID"</th>
                                                    <th>"Source"</th>
                                                    <th style="text-align: right;">"CPU %"</th>
                                                    <th style="text-align: right;">"Mémoire"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {if snap.processes.is_empty() {
                                                    view! {
                                                        <tr>
                                                            <td colspan="5" class="table-empty-cell">
                                                                "Aucun processus Riot actif détecté sur la machine."
                                                            </td>
                                                        </tr>
                                                    }.into_any()
                                                } else {
                                                    snap.processes.into_iter().map(|p| {
                                                        let (src_name, src_cls) = match p.source {
                                                            Some(SourceId::RiotClient) => ("Riot Client", "tag-rc"),
                                                            Some(SourceId::LeagueClient) => ("League Client", "tag-lcu"),
                                                            Some(SourceId::LiveGame) => ("Partie", "tag-jeu"),
                                                            Some(SourceId::System) => ("Système", "tag-sys"),
                                                            None => ("Riot Process", "tag-sys"),
                                                        };

                                                        view! {
                                                            <tr>
                                                                <td class="proc-name-cell" title=p.exe.unwrap_or_default()>
                                                                    <strong>{p.name}</strong>
                                                                </td>
                                                                <td class="tabular-nums font-mono">{p.pid}</td>
                                                                <td>
                                                                    <span class=format!("src-tag {}", src_cls)>{src_name}</span>
                                                                </td>
                                                                <td class="tabular-nums font-mono" style="text-align: right;">
                                                                    {format!("{:.1}%", p.cpu)}
                                                                </td>
                                                                <td class="tabular-nums font-mono" style="text-align: right;">
                                                                    {format_bytes(p.mem_bytes)}
                                                                </td>
                                                            </tr>
                                                        }
                                                    }).collect::<Vec<_>>().into_any()
                                                }}
                                            </tbody>
                                        </table>
                                    </div>
                                </section>

                                <section class="system-card" aria-labelledby="sock-table-title">
                                    <header class="system-card-header">
                                        <div class="card-header-left">
                                            <h2 id="sock-table-title" class="system-card-title">"PORTS EN ÉCOUTE"</h2>
                                            <span class="card-badge tabular-nums">{sock_count}</span>
                                        </div>
                                    </header>

                                    <div class="system-table-wrap">
                                        <table class="system-data-table" role="table">
                                            <thead>
                                                <tr>
                                                    <th>"Processus / PID"</th>
                                                    <th>"IP Locale"</th>
                                                    <th>"Port"</th>
                                                    <th>"Protocole"</th>
                                                    <th>"Source"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {if snap.sockets.is_empty() {
                                                    view! {
                                                        <tr>
                                                            <td colspan="5" class="table-empty-cell">
                                                                "Aucun socket en écoute actif."
                                                            </td>
                                                        </tr>
                                                    }.into_any()
                                                } else {
                                                    snap.sockets.into_iter().map(|s| {
                                                        let (proc_name, opt_source) = pid_map.get(&s.pid).cloned().unwrap_or_else(|| ("Inconnu".to_string(), None));
                                                        let (src_name, src_cls) = match opt_source {
                                                            Some(SourceId::RiotClient) => ("Riot Client", "tag-rc"),
                                                            Some(SourceId::LeagueClient) => ("League Client", "tag-lcu"),
                                                            Some(SourceId::LiveGame) => ("Partie", "tag-jeu"),
                                                            Some(SourceId::System) => ("Système", "tag-sys"),
                                                            None => ("Local", "tag-sys"),
                                                        };

                                                        view! {
                                                            <tr>
                                                                <td>
                                                                    <span>{proc_name} </span>
                                                                    <span class="tabular-nums font-mono text-muted">{format!("(:{})", s.pid)}</span>
                                                                </td>
                                                                <td class="tabular-nums font-mono">{s.local}</td>
                                                                <td class="tabular-nums font-mono font-bold" style="color: var(--src-live);">
                                                                    {s.port}
                                                                </td>
                                                                <td class="font-mono">{s.proto.to_uppercase()}</td>
                                                                <td>
                                                                    <span class=format!("src-tag {}", src_cls)>{src_name}</span>
                                                                </td>
                                                            </tr>
                                                        }
                                                    }).collect::<Vec<_>>().into_any()
                                                }}
                                            </tbody>
                                        </table>
                                    </div>
                                </section>

                                <section class="system-card" aria-labelledby="conn-table-title">
                                    <header class="system-card-header">
                                        <div class="card-header-left">
                                            <h2 id="conn-table-title" class="system-card-title">"CONNEXIONS SORTANTES"</h2>
                                            <span class="card-badge tabular-nums">{conn_count}</span>
                                        </div>
                                    </header>

                                    <div class="system-table-wrap">
                                        <table class="system-data-table" role="table">
                                            <thead>
                                                <tr>
                                                    <th>"Processus / PID"</th>
                                                    <th>"Adresse Locale"</th>
                                                    <th>"Adresse Distante"</th>
                                                    <th>"État TCP"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {if snap.connections.is_empty() {
                                                    view! {
                                                        <tr>
                                                            <td colspan="4" class="table-empty-cell">
                                                                "Aucune connexion TCP distante active."
                                                            </td>
                                                        </tr>
                                                    }.into_any()
                                                } else {
                                                    snap.connections.into_iter().map(|c| {
                                                        let (proc_name, _) = pid_map.get(&c.pid).cloned().unwrap_or_else(|| ("Inconnu".to_string(), None));

                                                        view! {
                                                            <tr>
                                                                <td>
                                                                    <span>{proc_name} </span>
                                                                    <span class="tabular-nums font-mono text-muted">{format!("(:{})", c.pid)}</span>
                                                                </td>
                                                                <td class="tabular-nums font-mono">{c.local}</td>
                                                                <td class="tabular-nums font-mono font-bold" style="color: var(--src-lcu);">
                                                                    {c.remote}
                                                                </td>
                                                                <td>
                                                                    <span class="tcp-state-tag">{c.state}</span>
                                                                </td>
                                                            </tr>
                                                        }
                                                    }).collect::<Vec<_>>().into_any()
                                                }}
                                            </tbody>
                                        </table>
                                    </div>
                                </section>
                            </div>
                        }.into_any()
                    }
                    None => {
                        view! {
                            <div class="system-loading-box">
                                <span class="spinner-dot"></span>
                                <span>"Chargement des processus et connexions système..."</span>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </main>
    }
}
