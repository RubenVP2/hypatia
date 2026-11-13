//! Process-wide "something observable changed" signal.
//!
//! Producers (event store, discovery, asset cache, language switch) bump it; consumers
//! (desktop UI bridge, Mobile Remote broadcaster) await it instead of polling on a timer,
//! so an idle application performs no periodic work at all.

use std::sync::Arc;

use tokio::sync::watch;

#[derive(Clone)]
pub struct ChangeSignal {
    tx: Arc<watch::Sender<u64>>,
}

impl Default for ChangeSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeSignal {
    pub fn new() -> Self {
        Self {
            tx: Arc::new(watch::channel(0).0),
        }
    }

    /// Marks observable state as changed and wakes every subscriber.
    pub fn bump(&self) {
        self.tx.send_modify(|rev| *rev = rev.wrapping_add(1));
    }

    /// Monotonic revision; equal revisions guarantee nothing observable changed in between.
    pub fn revision(&self) -> u64 {
        *self.tx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.tx.subscribe()
    }
}
