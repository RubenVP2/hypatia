use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock, RwLock};
use hypatia_shared::SecretRef;
use regex::Regex;

static SENSITIVE_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(token|password|passwd|secret|authorization|auth|jwt|cookie|session|credential|signature|refresh|access_?token|id_?token|entitlements?_?token)").unwrap()
});

static JWT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^eyJ[\w-]+\.[\w-]+\.[\w-]+$").unwrap()
});

/// Thread-safe storage for redacted secrets with LRU/bounded eviction.
#[derive(Clone)]
pub struct SecretVault {
    inner: Arc<RwLock<VaultInner>>,
}

struct VaultInner {
    entries: HashMap<u64, String>,
    order: VecDeque<u64>,
    next_id: u64,
    max_entries: usize,
}

impl Default for SecretVault {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretVault {
    pub const DEFAULT_MAX_ENTRIES: usize = 2000;

    /// Creates a new vault with default capacity (2000 entries).
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_MAX_ENTRIES)
    }

    /// Creates a new vault with a specific capacity.
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(VaultInner {
                entries: HashMap::with_capacity(max_entries.min(256)),
                order: VecDeque::with_capacity(max_entries.min(256)),
                next_id: 1,
                max_entries,
            })),
        }
    }

    /// Stores a secret string and returns a `SecretRef` containing its identifier, length, and hint.
    pub fn store(&self, secret: String, hint: String) -> SecretRef {
        let mut inner = self.inner.write().unwrap();
        let id = inner.next_id;
        inner.next_id = inner.next_id.wrapping_add(1);
        if inner.next_id == 0 {
            inner.next_id = 1;
        }

        let len = secret.len() as u32;

        if inner.max_entries > 0 && inner.order.len() >= inner.max_entries {
            if let Some(oldest_id) = inner.order.pop_front() {
                inner.entries.remove(&oldest_id);
            }
        }

        inner.order.push_back(id);
        inner.entries.insert(id, secret);

        SecretRef { id, len, hint }
    }

    /// Retrieves the original secret for a given identifier, if it is still stored in the vault.
    pub fn reveal(&self, id: u64) -> Option<String> {
        let inner = self.inner.read().unwrap();
        inner.entries.get(&id).cloned()
    }

    /// Returns the number of currently stored secrets.
    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.entries.len()
    }

    /// Returns true if the vault contains no secrets.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Checks whether a key is an exempt game/entity identifier that must NEVER be redacted.
pub fn is_exempt_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "puuid"
            | "summonerid"
            | "summoner_id"
            | "accountid"
            | "account_id"
            | "gameid"
            | "game_id"
            | "championid"
            | "champion_id"
            | "spellid"
            | "spell_id"
            | "iconid"
            | "icon_id"
            | "profileiconid"
            | "profile_icon_id"
            | "skinid"
            | "skin_id"
            | "itemid"
            | "item_id"
            | "teamid"
            | "team_id"
            | "partyid"
            | "party_id"
            | "lobbyid"
            | "lobby_id"
            | "cellid"
            | "cell_id"
            | "queueid"
            | "queue_id"
            | "mapid"
            | "map_id"
            | "platformid"
            | "platform_id"
    )
}

/// Checks whether a key matches sensitive patterns.
pub fn is_sensitive_key(key: &str) -> bool {
    if is_exempt_key(key) {
        return false;
    }
    SENSITIVE_KEY_RE.is_match(key)
}

/// Determines the most appropriate hint for a secret.
fn determine_hint(key: Option<&str>, is_jwt: bool) -> String {
    if is_jwt {
        return "jwt".to_string();
    }
    if let Some(k) = key {
        let lower = k.to_ascii_lowercase();
        if lower.contains("password") || lower.contains("passwd") {
            return "password".to_string();
        }
        if lower.contains("jwt") {
            return "jwt".to_string();
        }
        if lower.contains("cookie") {
            return "cookie".to_string();
        }
        if lower.contains("session") {
            return "session".to_string();
        }
        if lower.contains("credential") {
            return "credential".to_string();
        }
        if lower.contains("auth") {
            return "auth".to_string();
        }
        if lower.contains("token") {
            return "token".to_string();
        }
        if lower.contains("signature") {
            return "signature".to_string();
        }
        if lower.contains("refresh") {
            return "refresh".to_string();
        }
        if lower.contains("secret") {
            return "secret".to_string();
        }
    }
    "secret".to_string()
}

/// Redacts sensitive keys and values in-place within a JSON value.
///
/// Replaces sensitive strings with:
/// `{"$secret": {"id": id, "len": len, "hint": hint}}`
///
/// Returns `true` if at least one secret was redacted.
pub fn redact_value(val: &mut serde_json::Value, vault: &SecretVault) -> bool {
    redact_recursive(val, None, vault)
}

fn redact_recursive(
    val: &mut serde_json::Value,
    parent_key: Option<&str>,
    vault: &SecretVault,
) -> bool {
    match val {
        serde_json::Value::Object(map) => {
            // If already a redacted secret, don't redact again
            if map.len() == 1 && map.contains_key("$secret") {
                return false;
            }

            let mut redacted = false;
            for (key, child) in map.iter_mut() {
                if is_exempt_key(key) {
                    continue;
                }
                if redact_recursive(child, Some(key), vault) {
                    redacted = true;
                }
            }
            redacted
        }
        serde_json::Value::Array(arr) => {
            let mut redacted = false;
            for child in arr.iter_mut() {
                if redact_recursive(child, parent_key, vault) {
                    redacted = true;
                }
            }
            redacted
        }
        serde_json::Value::String(s) => {
            let is_jwt = JWT_RE.is_match(s);
            let is_parent_sensitive = parent_key.map(is_sensitive_key).unwrap_or(false);

            if is_jwt || is_parent_sensitive {
                if s.is_empty() {
                    return false;
                }
                let hint = determine_hint(parent_key, is_jwt);
                let secret_ref = vault.store(s.clone(), hint);
                *val = serde_json::json!({
                    "$secret": {
                        "id": secret_ref.id,
                        "len": secret_ref.len,
                        "hint": secret_ref.hint,
                    }
                });
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Generates a single-line summary (<= max_len characters) of a JSON value.
pub fn generate_preview(val: &serde_json::Value, max_len: usize) -> String {
    let raw = match val {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
    };

    let single_line = raw.replace('\n', " ").replace('\r', "");
    let trimmed = single_line.trim();

    let char_count = trimmed.chars().count();
    if char_count <= max_len {
        trimmed.to_string()
    } else if max_len == 0 {
        String::new()
    } else if max_len == 1 {
        "…".to_string()
    } else {
        let mut preview: String = trimmed.chars().take(max_len - 1).collect();
        preview.push('…');
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // 1. Isolated JWT test
    #[test]
    fn test_isolated_jwt() {
        let vault = SecretVault::new();
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let mut val = json!(jwt);

        assert!(redact_value(&mut val, &vault));

        let secret_obj = val.get("$secret").expect("$secret field expected");
        assert_eq!(secret_obj["hint"], "jwt");
        let id = secret_obj["id"].as_u64().unwrap();
        assert_eq!(secret_obj["len"], jwt.len() as u64);

        let revealed = vault.reveal(id);
        assert_eq!(revealed, Some(jwt.to_string()));
    }

    // 2. Nested object test
    #[test]
    fn test_nested_object() {
        let vault = SecretVault::new();
        let mut val = json!({
            "service": "riot-auth",
            "nested": {
                "level2": {
                    "token": "secret_deep_token_123",
                    "label": "public-label"
                }
            }
        });

        assert!(redact_value(&mut val, &vault));

        // The token is redacted
        let token_field = &val["nested"]["level2"]["token"];
        assert!(token_field.get("$secret").is_some());
        assert_eq!(token_field["$secret"]["hint"], "token");

        // The public label is not redacted
        assert_eq!(val["nested"]["level2"]["label"], "public-label");

        let id = token_field["$secret"]["id"].as_u64().unwrap();
        assert_eq!(vault.reveal(id), Some("secret_deep_token_123".to_string()));
    }

    // 3. Array of objects with secrets test
    #[test]
    fn test_array_of_objects_with_secrets() {
        let vault = SecretVault::new();
        let mut val = json!([
            {"user": "alice", "token": "tok_alice_123"},
            {"user": "bob", "token": "tok_bob_456"}
        ]);

        assert!(redact_value(&mut val, &vault));

        assert_eq!(val[0]["user"], "alice");
        assert!(val[0]["token"].get("$secret").is_some());

        assert_eq!(val[1]["user"], "bob");
        assert!(val[1]["token"].get("$secret").is_some());

        let id1 = val[0]["token"]["$secret"]["id"].as_u64().unwrap();
        let id2 = val[1]["token"]["$secret"]["id"].as_u64().unwrap();
        assert_ne!(id1, id2);

        assert_eq!(vault.reveal(id1), Some("tok_alice_123".to_string()));
        assert_eq!(vault.reveal(id2), Some("tok_bob_456".to_string()));
    }

    // 4. accessToken field test
    #[test]
    fn test_access_token_field() {
        let vault = SecretVault::new();
        let mut val = json!({
            "accessToken": "ey_access_token_value",
            "expiresIn": 3600
        });

        assert!(redact_value(&mut val, &vault));

        assert!(val["accessToken"].get("$secret").is_some());
        assert_eq!(val["accessToken"]["$secret"]["hint"], "token");
        assert_eq!(val["expiresIn"], 3600);
    }

    // 5. False positives test: puuid, summonerId, accountId remain intact
    #[test]
    fn test_false_positives_preserved() {
        let vault = SecretVault::new();
        let original = json!({
            "puuid": "00000000-0000-4000-8000-000000000001",
            "summonerId": 12345678,
            "accountId": 98765432,
            "gameId": 11223344,
            "championId": 103,
            "spellId": 4,
            "iconId": 29,
            "skinId": 103001,
            "itemId": 3031,
            "teamId": 100,
            "lobbyId": "EUW1_12345"
        });

        let mut val = original.clone();
        let redacted = redact_value(&mut val, &vault);

        assert!(!redacted, "No field should have been redacted");
        assert_eq!(val, original, "Object must remain strictly identical");
    }

    // 6. Reveal test via vault.reveal(ref.id) returning exact original string
    #[test]
    fn test_vault_reveal_exact_string() {
        let vault = SecretVault::new();
        let secret_text = "p@$$w0rd_with_Special_chars!&é\"'(§è!çà)";
        let sref = vault.store(secret_text.to_string(), "password".to_string());

        assert_eq!(vault.reveal(sref.id), Some(secret_text.to_string()));
        assert_eq!(vault.reveal(99999), None);
    }

    // 7. Test generate_preview (verifies truncation and readability)
    #[test]
    fn test_generate_preview() {
        // Short JSON
        let val_short = json!({"status": "ok"});
        let preview_short = generate_preview(&val_short, 120);
        assert_eq!(preview_short, r#"{"status":"ok"}"#);
        assert!(preview_short.chars().count() <= 120);

        // Long JSON exceeds 120 characters
        let val_long = json!({
            "long_key_name_with_lots_of_data": "This is a very long string that should be truncated when generating the single line preview for the event table so it fits comfortably",
            "another_field": 42
        });
        let preview_long = generate_preview(&val_long, 60);
        assert_eq!(preview_long.chars().count(), 60);
        assert!(preview_long.ends_with('…'));
        assert!(!preview_long.contains('\n'));

        // Multiline string
        let val_multiline = json!("Line1\nLine2\r\nLine3");
        let preview_multi = generate_preview(&val_multiline, 120);
        assert!(!preview_multi.contains('\n'));
        assert!(!preview_multi.contains('\r'));
    }

    // 8. password or credential key test
    #[test]
    fn test_password_and_credential_keys() {
        let vault = SecretVault::new();
        let mut val = json!({
            "password": "my_super_secret_password",
            "credential": "cred_xyz_12345",
            "passwd": "quick_passwd"
        });

        assert!(redact_value(&mut val, &vault));

        assert!(val["password"].get("$secret").is_some());
        assert_eq!(val["password"]["$secret"]["hint"], "password");

        assert!(val["credential"].get("$secret").is_some());
        assert_eq!(val["credential"]["$secret"]["hint"], "credential");

        assert!(val["passwd"].get("$secret").is_some());
        assert_eq!(val["passwd"]["$secret"]["hint"], "password");
    }

    // 9. Test vault LRU eviction
    #[test]
    fn test_vault_lru_eviction() {
        let vault = SecretVault::with_capacity(3);
        let s1 = vault.store("secret1".to_string(), "token".to_string());
        let s2 = vault.store("secret2".to_string(), "token".to_string());
        let s3 = vault.store("secret3".to_string(), "token".to_string());

        assert_eq!(vault.len(), 3);
        assert_eq!(vault.reveal(s1.id), Some("secret1".to_string()));

        // Storing a 4th secret evicts the oldest (s1)
        let s4 = vault.store("secret4".to_string(), "token".to_string());
        assert_eq!(vault.len(), 3);

        assert_eq!(vault.reveal(s1.id), None);
        assert_eq!(vault.reveal(s2.id), Some("secret2".to_string()));
        assert_eq!(vault.reveal(s3.id), Some("secret3".to_string()));
        assert_eq!(vault.reveal(s4.id), Some("secret4".to_string()));
    }

    // 10. Test no double redaction
    #[test]
    fn test_no_double_redaction() {
        let vault = SecretVault::new();
        let mut val = json!({
            "token": "tok_123"
        });

        assert!(redact_value(&mut val, &vault));
        let first_id = val["token"]["$secret"]["id"].as_u64().unwrap();

        // Second pass should return false and not mutate the secret object
        assert!(!redact_value(&mut val, &vault));
        assert_eq!(val["token"]["$secret"]["id"].as_u64().unwrap(), first_id);
    }
}
