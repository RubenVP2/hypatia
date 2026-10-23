use leptos::prelude::*;

#[component]
pub fn SystemView() -> impl IntoView {
    view! {
        <main class="live-view-container" style="justify-content: center; align-items: center;">
            <div class="empty-state">
                <div class="empty-state-icon">"⚡"</div>
                <h2 style="font-family: var(--font-display); font-size: var(--fs-xl); color: var(--text);">
                    "État système & Processus Riot"
                </h2>
                <div class="empty-state-text">
                    "Processus locaux, sockets TCP en écoute et connexions distantes chiffrées (Phase 7)."
                </div>
            </div>
        </main>
    }
}
