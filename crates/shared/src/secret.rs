use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    pub id: u64,
    pub len: u32,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedSecret {
    #[serde(rename = "$secret")]
    pub secret: SecretRef,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_ref_serde() {
        let secret = SecretRef {
            id: 42,
            len: 128,
            hint: "token".to_string(),
        };
        let json = serde_json::to_string(&secret).expect("serialize secret");
        assert_eq!(json, r#"{"id":42,"len":128,"hint":"token"}"#);

        let redacted = RedactedSecret {
            secret: secret.clone(),
        };
        let red_json = serde_json::to_string(&redacted).expect("serialize redacted");
        assert_eq!(
            red_json,
            r#"{"$secret":{"id":42,"len":128,"hint":"token"}}"#
        );

        let back: RedactedSecret = serde_json::from_str(&red_json).expect("deserialize redacted");
        assert_eq!(back.secret, secret);
    }
}
