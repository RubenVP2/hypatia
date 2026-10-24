use leptos::prelude::*;
use crate::state::AppState;

#[component]
pub fn PhaseTimeline() -> impl IntoView {
    let state = expect_context::<AppState>();
    let segments_sig = state.phase_store.segments;
    let current_phase_sig = state.phase_store.current;

    let segments_data = move || {
        let mut list = segments_sig.get();

        // If a phase is in progress, add it as the active segment
        if let Some(curr) = current_phase_sig.get() {
            let now = js_sys::Date::now() as i64;
            let dur = (now.saturating_sub(curr.since)).max(1000) as u64;
            list.push(crate::state::PhaseSegment {
                name: curr.phase,
                start_ts: curr.since,
                duration_ms: dur,
            });
        }

        let total_ms: u64 = list.iter().map(|s| s.duration_ms).sum();
        let total_ms = total_ms.max(1);

        list.into_iter()
            .map(|seg| {
                let pct = (seg.duration_ms as f64 / total_ms as f64) * 100.0;
                let mins = seg.duration_ms / 60000;
                let secs = (seg.duration_ms % 60000) / 1000;
                let dur_str = format!("{:02}:{:02}", mins, secs);

                let color = match seg.name.as_str() {
                    "ChampSelect" => "var(--src-riot)",
                    "InProgress" => "var(--src-live)",
                    "Lobby" => "var(--src-lcu)",
                    "Matchmaking" => "var(--src-system)",
                    _ => "var(--n-5)",
                };

                (seg.name, pct, dur_str, color)
            })
            .collect::<Vec<_>>()
    };

    view! {
        <div class="phase-timeline-container" aria-label="Chronological phase history">
            <div class="timeline-bar">
                {move || {
                    let items = segments_data();
                    if items.is_empty() {
                        view! {
                            <div
                                class="timeline-segment"
                                style="width: 100%; background-color: var(--n-4);"
                                title="Waiting for phase data..."
                            ></div>
                        }.into_any()
                    } else {
                        items.into_iter().map(|(name, pct, dur, color)| {
                            let display_name = match name.as_str() {
                                "ChampSelect" => "Champion Select",
                                "InProgress" => "In Game",
                                "Lobby" => "Lobby",
                                "Matchmaking" => "In Queue",
                                "ReadyCheck" => "Match Found",
                                "WaitingForStats" => "Waiting for Stats",
                                "PreEndOfGame" | "EndOfGame" => "End of Game",
                                "Reconnect" => "Reconnect",
                                other => other,
                            };
                            let title = format!("{}: {}", display_name, dur);
                            let style = format!("width: {:.2}%; background-color: {};", pct.max(1.0), color);
                            view! {
                                <div
                                    class="timeline-segment"
                                    style=style
                                    title=title
                                ></div>
                            }
                        }).collect::<Vec<_>>().into_any()
                    }
                }}
            </div>

            <div class="timeline-legend">
                <div class="legend-item">
                    <span class="legend-swatch" style="background-color: var(--src-lcu);"></span>
                    <span>"Lobby"</span>
                </div>
                <div class="legend-item">
                    <span class="legend-swatch" style="background-color: var(--src-system);"></span>
                    <span>"In queue"</span>
                </div>
                <div class="legend-item">
                    <span class="legend-swatch" style="background-color: var(--src-riot);"></span>
                    <span>"Champ Select"</span>
                </div>
                <div class="legend-item">
                    <span class="legend-swatch" style="background-color: var(--src-live);"></span>
                    <span>"In game"</span>
                </div>
            </div>
        </div>
    }
}
