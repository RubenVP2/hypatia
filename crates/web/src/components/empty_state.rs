use leptos::prelude::*;
use hypatia_shared::SourceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EmptyStateType {
    RiotClientOffline,
    LeagueOffline,
    LiveGameOffline,
    ReplayOffline,
    WsDisconnected,
    NoEvents,
    Custom,
}

#[component]
pub fn EmptyState(
    #[prop(optional)] state_type: Option<EmptyStateType>,
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] message: Option<String>,
    #[prop(optional, into)] icon: Option<String>,
) -> impl IntoView {
    let t = state_type.unwrap_or(EmptyStateType::Custom);

    let (icon_str, title_str, message_str) = match t {
        EmptyStateType::RiotClientOffline => (
            "⏻".to_string(),
            "Riot Client offline".to_string(),
            "Riot Client is not running. The dashboard will connect automatically once it starts.".to_string(),
        ),
        EmptyStateType::LeagueOffline => (
            "⏻".to_string(),
            "League of Legends closed".to_string(),
            "League of Legends is not running.".to_string(),
        ),
        EmptyStateType::LiveGameOffline => (
            "🎮".to_string(),
            "No live game in progress".to_string(),
            "No live game in progress. Game client data (port 2999) will appear once a game launches.".to_string(),
        ),
        EmptyStateType::ReplayOffline => (
            "📹".to_string(),
            "Replay API inactive".to_string(),
            "Replay API inactive — add `EnableReplayApi=1` in `[General]` section of `game.cfg`.".to_string(),
        ),
        EmptyStateType::WsDisconnected => (
            "⚡".to_string(),
            "Connection interrupted".to_string(),
            "Lost connection to local server — reconnecting...".to_string(),
        ),
        EmptyStateType::NoEvents => (
            "⌕".to_string(),
            "No events".to_string(),
            "No events matching the active filters.".to_string(),
        ),
        EmptyStateType::Custom => (
            icon.unwrap_or_else(|| "⌕".to_string()),
            title.unwrap_or_else(|| "No data".to_string()),
            message.unwrap_or_else(|| "No item available.".to_string()),
        ),
    };

    view! {
        <div class="empty-state" role="status">
            <div class="empty-state-icon">{icon_str}</div>
            <h3 class="empty-state-heading">{title_str}</h3>
            <p class="empty-state-text">{message_str}</p>
        </div>
    }
}

/// Warning banner in case of lost WebSocket connection
#[component]
pub fn WsDisconnectedBanner() -> impl IntoView {
    view! {
        <div class="ws-disconnected-banner" role="alert" aria-live="assertive">
            <span class="pulse-warning-dot"></span>
            <span>"Lost connection to local server — reconnecting..."</span>
        </div>
    }
}

/// Helper to obtain the appropriate EmptyState for an offline source
pub fn source_offline_state(source: SourceId) -> EmptyStateType {
    match source {
        SourceId::RiotClient => EmptyStateType::RiotClientOffline,
        SourceId::LeagueClient => EmptyStateType::LeagueOffline,
        SourceId::LiveGame => EmptyStateType::LiveGameOffline,
        SourceId::System => EmptyStateType::Custom,
    }
}
