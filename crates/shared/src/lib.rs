pub mod board;
pub mod catalog;
pub mod champions;
pub mod event;
pub mod i18n;
pub mod protocol;
pub mod remote;
pub mod secret;
pub mod source;
pub mod summary;
pub mod system;

pub use board::*;
pub use catalog::*;
pub use champions::*;
pub use event::*;
pub use i18n::*;
pub use protocol::*;
pub use remote::*;
pub use secret::*;
pub use source::*;
pub use summary::*;
pub use system::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub demo: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let health = HealthResponse {
            status: "ok".to_string(),
            demo: false,
        };
        let json = serde_json::to_string(&health).expect("serialization failed");
        assert!(json.contains("\"status\":\"ok\""));
    }
}
