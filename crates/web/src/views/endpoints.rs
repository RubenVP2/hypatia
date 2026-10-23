use leptos::prelude::*;

#[component]
pub fn EndpointsView() -> impl IntoView {
    view! {
        <main class="live-view-container" style="justify-content: center; align-items: center;">
            <div class="empty-state">
                <div class="empty-state-icon">"⊞"</div>
                <h2 style="font-family: var(--font-display); font-size: var(--fs-xl); color: var(--text);">
                    "Catalogue des endpoints"
                </h2>
                <div class="empty-state-text">
                    "Exploration de plus de 500 endpoints découverts via /help et les schémas OpenAPI (Phase 7)."
                </div>
            </div>
        </main>
    }
}
