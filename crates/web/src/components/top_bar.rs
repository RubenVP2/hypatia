use leptos::prelude::*;
use hypatia_shared::SourceId;
use crate::components::source_chip::SourceChip;
use crate::components::sparkline::Sparkline;
use crate::state::{AppState, NavTab};

#[component]
pub fn TopBar() -> impl IntoView {
    let state = expect_context::<AppState>();

    let is_connected = state.is_connected;
    let active_tab = state.active_tab;
    let secrets_hidden = state.secrets_hidden;

    let toggle_secrets = move |_| {
        secrets_hidden.update(|v| *v = !*v);
    };

    view! {
        <header class="top-bar" role="banner">
            <div class="topbar-left">
                <div class="brand-title">
                    <span
                        class=move || {
                            format!(
                                "connection-dot {}",
                                if is_connected.get() { "online" } else { "offline" }
                            )
                        }
                        title=move || {
                            if is_connected.get() {
                                "Connecté au serveur local"
                            } else {
                                "Connexion au serveur local en cours..."
                            }
                        }
                    ></span>
                    <span>"HYPATIA"</span>
                </div>

                <div class="source-chips" role="toolbar" aria-label="Sources Riot">
                    <SourceChip id=SourceId::RiotClient />
                    <SourceChip id=SourceId::LeagueClient />
                    <SourceChip id=SourceId::LiveGame />
                    <SourceChip id=SourceId::System />
                </div>
            </div>

            <nav class="nav-tabs" role="tablist" aria-label="Vues principales">
                <button
                    class=move || {
                        format!("nav-tab {}", if active_tab.get() == NavTab::Live { "active" } else { "" })
                    }
                    role="tab"
                    aria-selected=move || (active_tab.get() == NavTab::Live).to_string()
                    on:click=move |_| active_tab.set(NavTab::Live)
                >
                    "En direct" <kbd>"1"</kbd>
                </button>
                <button
                    class=move || {
                        format!("nav-tab {}", if active_tab.get() == NavTab::Endpoints { "active" } else { "" })
                    }
                    role="tab"
                    aria-selected=move || (active_tab.get() == NavTab::Endpoints).to_string()
                    on:click=move |_| active_tab.set(NavTab::Endpoints)
                >
                    "Endpoints" <kbd>"2"</kbd>
                </button>
                <button
                    class=move || {
                        format!("nav-tab {}", if active_tab.get() == NavTab::System { "active" } else { "" })
                    }
                    role="tab"
                    aria-selected=move || (active_tab.get() == NavTab::System).to_string()
                    on:click=move |_| active_tab.set(NavTab::System)
                >
                    "Système" <kbd>"3"</kbd>
                </button>
                <button
                    class=move || {
                        format!("nav-tab {}", if active_tab.get() == NavTab::About { "active" } else { "" })
                    }
                    role="tab"
                    aria-selected=move || (active_tab.get() == NavTab::About).to_string()
                    on:click=move |_| active_tab.set(NavTab::About)
                >
                    "À propos" <kbd>"4"</kbd>
                </button>
            </nav>

            <div class="topbar-right">
                <Sparkline />

                <button
                    class="secrets-toggle-btn"
                    on:click=toggle_secrets
                    title="Les secrets (tokens, mots de passe) sont masqués par défaut."
                >
                    <span>{move || if secrets_hidden.get() { "🔒 Secrets masqués" } else { "🔓 Secrets visibles" }}</span>
                </button>

                <button
                    type="button"
                    class="kbd-badge palette-trigger-btn"
                    title="Palette de commandes (Ctrl+K)"
                    on:click=move |_| state.command_palette_open.set(true)
                >
                    "Ctrl+K"
                </button>
            </div>
        </header>
    }
}
