pub mod live_client;
pub mod replay;
pub mod wamp;

use std::sync::{Arc, RwLock};

pub use live_client::*;
pub use replay::*;
pub use wamp::*;

use crate::assets::GameAssetCache;
use crate::catalog::CatalogStore;
use crate::discovery::DiscoveryManager;
use crate::redaction::SecretVault;
use crate::riot_http::ReadOnlyRiotClient;
use crate::settings::Flags;
use crate::signal::ChangeSignal;
use crate::store::EventStore;

/// Everything a connector needs to observe a Riot source and record what it sees.
#[derive(Clone)]
pub struct ConnectorContext {
    pub discovery: Arc<DiscoveryManager>,
    pub store: Arc<RwLock<EventStore>>,
    pub catalog: Arc<RwLock<CatalogStore>>,
    pub assets: Arc<GameAssetCache>,
    pub vault: SecretVault,
    pub client: ReadOnlyRiotClient,
    pub flags: Arc<Flags>,
    pub signal: ChangeSignal,
}

impl ConnectorContext {
    /// Records traffic on `uri` in the endpoint catalog while the Telemetry Lab is open.
    fn observe(&self, source: hypatia_shared::SourceId, uri: &str, bytes: u64) {
        if self.flags.telemetry() {
            self.catalog
                .write()
                .unwrap()
                .record_observed(source, "GET", uri, bytes);
        }
    }
}
