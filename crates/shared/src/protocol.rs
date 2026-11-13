use serde::{Deserialize, Serialize};

use crate::event::{EventMeta, GamePhase};
use crate::source::SourceState;
use crate::system::SystemSnapshot;

/// Telemetry Observatory snapshot: sources, phase, newest event metadata and system footprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub sources: Vec<SourceState>,
    pub phase: Option<GamePhase>,
    pub recent_events: Vec<EventMeta>,
    pub system: SystemSnapshot,
}
