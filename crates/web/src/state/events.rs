use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use leptos::prelude::*;
use hypatia_shared::{EventKind, EventMeta, SourceId};

pub const MAX_EVENTS_CAPACITY: usize = 10_000;
pub const SPARKLINE_POINTS: usize = 60;

#[derive(Clone)]
pub struct EventStore {
    pub events: Arc<RwLock<VecDeque<EventMeta>>>,
    pub version: RwSignal<u64>,
    pub rate: RwSignal<f32>,
    pub rate_history: RwSignal<Vec<f32>>,
    pub is_paused: RwSignal<bool>,
    pub unseen_count: RwSignal<usize>,
    pub selected_id: RwSignal<Option<u64>>,
    pub selected_event: RwSignal<Option<EventMeta>>,
    pub filter_source: RwSignal<Option<SourceId>>,
    pub filter_kind: RwSignal<Option<EventKind>>,
    pub filter_query: RwSignal<String>,
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_EVENTS_CAPACITY))),
            version: RwSignal::new(0),
            rate: RwSignal::new(0.0),
            rate_history: RwSignal::new(vec![0.0; SPARKLINE_POINTS]),
            is_paused: RwSignal::new(false),
            unseen_count: RwSignal::new(0),
            selected_id: RwSignal::new(None),
            selected_event: RwSignal::new(None),
            filter_source: RwSignal::new(None),
            filter_kind: RwSignal::new(None),
            filter_query: RwSignal::new(String::new()),
        }
    }

    pub fn push_batch(&self, new_events: Vec<EventMeta>, current_rate: f32) {
        if new_events.is_empty() && current_rate == self.rate.get_untracked() {
            return;
        }

        let paused = self.is_paused.get_untracked();
        let count = new_events.len();

        if let Ok(mut deque) = self.events.write() {
            for ev in new_events {
                if deque.len() >= MAX_EVENTS_CAPACITY {
                    deque.pop_front();
                }
                deque.push_back(ev);
            }
        }

        // If paused, accumulate the count of new unseen events
        if paused {
            self.unseen_count.update(|c| *c += count);
        } else {
            // Increment version to refresh table
            self.version.update(|v| *v += 1);
        }

        self.rate.set(current_rate);

        // Update history for Sparkline
        self.rate_history.update(|hist| {
            if hist.len() >= SPARKLINE_POINTS {
                hist.remove(0);
            }
            hist.push(current_rate);
        });
    }

    pub fn init_recent(&self, recent: Vec<EventMeta>) {
        if let Ok(mut deque) = self.events.write() {
            deque.clear();
            for ev in recent {
                if deque.len() >= MAX_EVENTS_CAPACITY {
                    deque.pop_front();
                }
                deque.push_back(ev);
            }
        }
        self.version.update(|v| *v += 1);
        self.unseen_count.set(0);
    }

    pub fn toggle_pause(&self) {
        let currently_paused = self.is_paused.get_untracked();
        let next_paused = !currently_paused;
        self.is_paused.set(next_paused);

        if !next_paused {
            // When resuming from pause, synchronize the view
            self.unseen_count.set(0);
            self.version.update(|v| *v += 1);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut deque) = self.events.write() {
            deque.clear();
        }
        self.unseen_count.set(0);
        self.selected_id.set(None);
        self.selected_event.set(None);
        self.version.update(|v| *v += 1);
    }

    pub fn select_event(&self, id: u64) {
        self.selected_id.set(Some(id));
        if let Ok(deque) = self.events.read() {
            if let Some(ev) = deque.iter().find(|e| e.id == id) {
                self.selected_event.set(Some(ev.clone()));
            }
        }
    }

    /// Retrieves the list of filtered events
    pub fn get_filtered(&self) -> Vec<EventMeta> {
        let deque_guard = match self.events.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        let src_filter = self.filter_source.get();
        let kind_filter = self.filter_kind.get();
        let q = self.filter_query.get().to_lowercase();

        // If paused with new events, limit to events known when paused
        let limit = if self.is_paused.get() {
            let unseen = self.unseen_count.get();
            deque_guard.len().saturating_sub(unseen)
        } else {
            deque_guard.len()
        };

        deque_guard
            .iter()
            .take(limit)
            .filter(|ev| {
                if let Some(src) = src_filter {
                    if ev.source != src {
                        return false;
                    }
                }
                if let Some(kind) = kind_filter {
                    if ev.kind != kind {
                        return false;
                    }
                }
                if !q.is_empty() && !ev.uri.to_lowercase().contains(&q) {
                    return false;
                }
                true
            })
            .cloned()
            .collect()
    }

    /// Retrieves the last 30 events for the Ticker
    pub fn get_ticker_events(&self) -> Vec<EventMeta> {
        let deque_guard = match self.events.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let take_count = 30.min(deque_guard.len());
        deque_guard.iter().rev().take(take_count).cloned().collect()
    }
}
