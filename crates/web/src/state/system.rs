use hypatia_shared::SystemSnapshot;
use leptos::prelude::*;

#[derive(Clone)]
pub struct SystemStore {
    pub snapshot: RwSignal<Option<SystemSnapshot>>,
    pub is_loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl SystemStore {
    pub fn new() -> Self {
        Self {
            snapshot: RwSignal::new(None),
            is_loading: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn set_snapshot(&self, snap: SystemSnapshot) {
        self.snapshot.set(Some(snap));
        self.is_loading.set(false);
        self.error.set(None);
    }
}
