use leptos::prelude::*;
use crate::state::AppState;

#[component]
pub fn Sparkline() -> impl IntoView {
    let state = expect_context::<AppState>();
    let rate_sig = state.event_store.rate;
    let hist_sig = state.event_store.rate_history;

    let points_str = move || {
        let history = hist_sig.get();
        if history.is_empty() {
            return "0,24 90,24".to_string();
        }

        let max_val = history.iter().cloned().fold(10.0f32, f32::max);
        let width = 90.0f32;
        let height = 24.0f32;
        let step = width / (history.len().max(1) - 1) as f32;

        let mut pts = String::new();
        for (i, val) in history.iter().enumerate() {
            let x = i as f32 * step;
            let norm = (val / max_val).clamp(0.0, 1.0);
            let y = height - (norm * (height - 4.0)) - 2.0;
            pts.push_str(&format!("{:.1},{:.1} ", x, y));
        }
        pts
    };

    let rate_display = move || {
        let r = rate_sig.get();
        if r >= 100.0 {
            format!("{:.0}", r)
        } else {
            format!("{:.1}", r)
        }
    };

    view! {
        <div class="sparkline-container" title="Débit d'événements sur les 60 dernières secondes">
            <svg class="sparkline-svg" viewBox="0 0 90 24" preserveAspectRatio="none">
                <polyline
                    fill="none"
                    stroke="var(--src-lcu)"
                    stroke-width="1.5"
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    points=points_str
                />
            </svg>
            <span class="sparkline-rate tabular-nums">{rate_display}</span>
            <span class="sparkline-unit">"évt/s"</span>
        </div>
    }
}
