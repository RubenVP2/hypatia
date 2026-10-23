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
            "Riot Client inactif".to_string(),
            "Le Riot Client n'est pas lancé. Le tableau de bord se connectera tout seul dès qu'il démarre.".to_string(),
        ),
        EmptyStateType::LeagueOffline => (
            "⏻".to_string(),
            "League of Legends fermé".to_string(),
            "League of Legends n'est pas ouvert.".to_string(),
        ),
        EmptyStateType::LiveGameOffline => (
            "🎮".to_string(),
            "Aucune partie en cours".to_string(),
            "Aucune partie en cours. Les données du jeu (port 2999) apparaîtront au lancement d'une partie.".to_string(),
        ),
        EmptyStateType::ReplayOffline => (
            "📹".to_string(),
            "Replay API inactive".to_string(),
            "Replay API désactivée — ajoute `EnableReplayApi=1` dans la section `[General]` de `game.cfg`.".to_string(),
        ),
        EmptyStateType::WsDisconnected => (
            "⚡".to_string(),
            "Connexion interrompue".to_string(),
            "Connexion au serveur local perdue — nouvelle tentative...".to_string(),
        ),
        EmptyStateType::NoEvents => (
            "⌕".to_string(),
            "Aucun événement".to_string(),
            "Aucun événement correspondant aux filtres actifs.".to_string(),
        ),
        EmptyStateType::Custom => (
            icon.unwrap_or_else(|| "⌕".to_string()),
            title.unwrap_or_else(|| "Aucune donnée".to_string()),
            message.unwrap_or_else(|| "Aucun élément disponible.".to_string()),
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

/// Bandeau d'alerte en cas de perte de connexion WebSocket
#[component]
pub fn WsDisconnectedBanner() -> impl IntoView {
    view! {
        <div class="ws-disconnected-banner" role="alert" aria-live="assertive">
            <span class="pulse-warning-dot"></span>
            <span>"Connexion au serveur local perdue — nouvelle tentative..."</span>
        </div>
    }
}

/// Helper pour obtenir l'EmptyState approprié pour une source offline
pub fn source_offline_state(source: SourceId) -> EmptyStateType {
    match source {
        SourceId::RiotClient => EmptyStateType::RiotClientOffline,
        SourceId::LeagueClient => EmptyStateType::LeagueOffline,
        SourceId::LiveGame => EmptyStateType::LiveGameOffline,
        SourceId::System => EmptyStateType::Custom,
    }
}
