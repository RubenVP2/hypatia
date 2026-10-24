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
                                "Connected to local server"
                            } else {
                                "Connecting to local server..."
                            }
                        }
                    ></span>
                    <span>"HYPATIA"</span>
                </div>

                <div class="source-chips" role="toolbar" aria-label="Riot Sources">
                    <SourceChip id=SourceId::RiotClient />
                    <SourceChip id=SourceId::LeagueClient />
                    <SourceChip id=SourceId::LiveGame />
                    <SourceChip id=SourceId::System />
                </div>
            </div>

            <nav class="nav-tabs" role="tablist" aria-label="Main views">
                <button
                    class=move || {
                        format!("nav-tab {}", if active_tab.get() == NavTab::Live { "active" } else { "" })
                    }
                    role="tab"
                    aria-selected=move || (active_tab.get() == NavTab::Live).to_string()
                    on:click=move |_| active_tab.set(NavTab::Live)
                >
                    "Live" <kbd>"1"</kbd>
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
                    "System" <kbd>"3"</kbd>
                </button>
                <button
                    class=move || {
                        format!("nav-tab {}", if active_tab.get() == NavTab::About { "active" } else { "" })
                    }
                    role="tab"
                    aria-selected=move || (active_tab.get() == NavTab::About).to_string()
                    on:click=move |_| active_tab.set(NavTab::About)
                >
                    "About" <kbd>"4"</kbd>
                </button>
            </nav>

            <div class="topbar-right">
                <Sparkline />

                <button
                    class="secrets-toggle-btn"
                    on:click=toggle_secrets
                    title="Secrets (tokens, passwords) are masked by default."
                >
                    <span>{move || if secrets_hidden.get() { "🔒 Secrets masked" } else { "🔓 Secrets visible" }}</span>
                </button>

                <button
                    type="button"
                    class="kbd-badge palette-trigger-btn"
                    title="Command palette (Ctrl+K)"
                    on:click=move |_| state.command_palette_open.set(true)
                >
                    "Ctrl+K"
                </button>
            </div>
        </header>
    }
}
