use leptos::prelude::*;
use crate::components::{EventTable, PhaseCard, PhaseTimeline};

#[component]
pub fn LiveView() -> impl IntoView {
    view! {
        <main class="live-view-container" role="main" aria-label="Vue En direct">
            <PhaseCard />
            <PhaseTimeline />
            <EventTable />
        </main>
    }
}
