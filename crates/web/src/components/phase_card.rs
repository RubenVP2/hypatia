use leptos::prelude::*;
use gloo_timers::future::TimeoutFuture;
use crate::state::AppState;

#[component]
pub fn PhaseCard() -> impl IntoView {
    let state = expect_context::<AppState>();
    let current_phase_sig = state.phase_store.current;
    let prev_phase_sig = state.phase_store.previous_name;

    // Signal pour tick du timer chaque seconde
    let tick = RwSignal::new(0u64);

    // Boucle d'incrémentation du tick
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            TimeoutFuture::new(1000).await;
            tick.update(|t| *t += 1);
        }
    });

    let phase_display_name = move |p_str: &str| -> &'static str {
        match p_str {
            "None" => "AUCUNE ACTIVITÉ",
            "Lobby" => "SALON",
            "Matchmaking" => "EN FILE",
            "ReadyCheck" => "PARTIE TROUVÉE",
            "ChampSelect" => "SÉLECTION DES CHAMPIONS",
            "InProgress" => "EN JEU",
            "WaitingForStats" => "ATTENTE DES STATS",
            "PreEndOfGame" | "EndOfGame" => "FIN DE PARTIE",
            "Reconnect" => "RECONNEXION",
            _ => "SALON",
        }
    };

    let elapsed_formatted = move || {
        let _ = tick.get(); // dépendance réactive pour rafraîchir chaque seconde
        if let Some(phase) = current_phase_sig.get() {
            let now = (js_sys::Date::now() / 1000.0) as i64;
            let start = phase.since / 1000;
            let elapsed_sec = (now.saturating_sub(start)).max(0);

            let mins = elapsed_sec / 60;
            let secs = elapsed_sec % 60;
            if mins >= 60 {
                let hours = mins / 60;
                let rem_mins = mins % 60;
                format!("{:02}:{:02}:{:02}", hours, rem_mins, secs)
            } else {
                format!("{:02}:{:02}", mins, secs)
            }
        } else {
            "00:00".to_string()
        }
    };

    let current_name = move || {
        current_phase_sig
            .get()
            .map(|p| phase_display_name(&p.phase).to_string())
            .unwrap_or_else(|| "AUCUNE ACTIVITÉ".to_string())
    };

    let prev_display = move || {
        prev_phase_sig.get().map(|prev| {
            format!("Précédent : {}", phase_display_name(&prev))
        })
    };

    view! {
        <section class="phase-card chamfer" aria-label="État actuel du client de jeu">
            <div class="phase-info">
                <h1 class="phase-name">{current_name}</h1>
                {move || prev_display().map(|txt| {
                    view! { <span class="phase-prev">{txt}</span> }
                })}
            </div>

            <div class="phase-timer-box">
                <span class="phase-timer tabular-nums">{elapsed_formatted}</span>
                <span class="phase-timer-lbl">"Écoulé"</span>
            </div>
        </section>
    }
}
