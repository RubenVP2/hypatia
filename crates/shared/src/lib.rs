pub mod catalog;
pub mod event;
pub mod protocol;
pub mod secret;
pub mod source;
pub mod system;

pub use catalog::*;
pub use event::*;
pub use protocol::*;
pub use secret::*;
pub use source::*;
pub use system::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let health = HealthResponse {
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&health).expect("serialization failed");
        assert_eq!(json, r#"{"status":"ok"}"#);
    }
}
