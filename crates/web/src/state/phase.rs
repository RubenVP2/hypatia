use leptos::prelude::*;
use hypatia_shared::GamePhase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseSegment {
    pub name: String,
    pub start_ts: i64,
    pub duration_ms: u64,
}

#[derive(Clone)]
pub struct PhaseStore {
    pub current: RwSignal<Option<GamePhase>>,
    pub previous_name: RwSignal<Option<String>>,
    pub segments: RwSignal<Vec<PhaseSegment>>,
}

impl PhaseStore {
    pub fn new() -> Self {
        Self {
            current: RwSignal::new(None),
            previous_name: RwSignal::new(None),
            segments: RwSignal::new(Vec::new()),
        }
    }

    pub fn set_phase(&self, new_phase: GamePhase) {
        let prev = self.current.get_untracked();
        let now = js_sys::Date::now() as i64;

        if let Some(old) = prev {
            if old.phase == new_phase.phase {
                return;
            }
            self.previous_name.set(Some(old.phase.clone()));

            // Enregistrer le segment terminé
            let duration = (now.saturating_sub(old.since)) as u64;
            self.segments.update(|segs| {
                segs.push(PhaseSegment {
                    name: old.phase,
                    start_ts: old.since,
                    duration_ms: duration.max(1000), // au moins 1s
                });
            });
        }

        self.current.set(Some(new_phase));
    }
}
