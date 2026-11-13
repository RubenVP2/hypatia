use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, RwLock};

use hypatia_shared::SecretRef;
use serde_json::Value;

/// Key fragments marking a JSON string value as a credential (matched ASCII case-insensitively).
const SENSITIVE_KEY_FRAGMENTS: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "auth",
    "jwt",
    "cookie",
    "session",
    "credential",
    "signature",
    "refresh",
];

/// Game/entity identifiers that must never be redacted even though they look sensitive.
const EXEMPT_KEYS: &[&str] = &[
    "puuid",
    "summonerid",
    "summoner_id",
    "accountid",
    "account_id",
    "gameid",
    "game_id",
    "championid",
    "champion_id",
    "spellid",
    "spell_id",
    "iconid",
    "icon_id",
    "profileiconid",
    "profile_icon_id",
    "skinid",
    "skin_id",
    "itemid",
    "item_id",
    "teamid",
    "team_id",
    "partyid",
    "party_id",
    "lobbyid",
    "lobby_id",
    "cellid",
    "cell_id",
    "queueid",
    "queue_id",
    "mapid",
    "map_id",
    "platformid",
    "platform_id",
];

/// Thread-safe bounded storage for redacted secrets (oldest entries evicted first).
///
/// Storing the same secret twice yields the same identifier, so credentials re-read on every
/// discovery scan neither churn their `SecretRef` nor push real secrets out of the vault.
#[derive(Clone)]
pub struct SecretVault {
    inner: Arc<RwLock<VaultInner>>,
}

struct VaultInner {
    entries: HashMap<u64, Arc<str>>,
    ids: HashMap<Arc<str>, u64>,
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

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_MAX_ENTRIES)
    }

    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(VaultInner {
                entries: HashMap::new(),
                ids: HashMap::new(),
                order: VecDeque::new(),
                next_id: 1,
                max_entries,
            })),
        }
    }

    /// Stores a secret and returns a `SecretRef` carrying its identifier, length and hint.
    pub fn store(&self, secret: String, hint: String) -> SecretRef {
        let len = secret.len() as u32;
        let mut inner = self.inner.write().unwrap();
        if let Some(&id) = inner.ids.get(secret.as_str()) {
            return SecretRef { id, len, hint };
        }

        let id = inner.next_id;
        inner.next_id = inner.next_id.checked_add(1).unwrap_or(1);

        if inner.max_entries > 0 && inner.order.len() >= inner.max_entries {
            if let Some(oldest) = inner.order.pop_front() {
                if let Some(evicted) = inner.entries.remove(&oldest) {
                    inner.ids.remove(&evicted);
                }
            }
        }

        let secret: Arc<str> = secret.into();
        inner.order.push_back(id);
        inner.ids.insert(Arc::clone(&secret), id);
        inner.entries.insert(id, secret);
        SecretRef { id, len, hint }
    }

    /// Returns the original secret for `id` if it is still in the vault.
    pub fn reveal(&self, id: u64) -> Option<String> {
        self.inner.read().unwrap().entries.get(&id).map(|s| s.to_string())
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Checks whether a key is an exempt game/entity identifier that must NEVER be redacted.
pub fn is_exempt_key(key: &str) -> bool {
    EXEMPT_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key))
}

/// Checks whether a key names a credential.
pub fn is_sensitive_key(key: &str) -> bool {
    !is_exempt_key(key)
        && SENSITIVE_KEY_FRAGMENTS
            .iter()
            .any(|f| contains_ignore_ascii_case(key, f))
}

/// True for a compact JWS: three non-empty base64url segments, the first starting with `eyJ`.
fn is_jwt(s: &str) -> bool {
    if !s.starts_with("eyJ") {
        return false;
    }
    let mut segments = 0;
    for segment in s.split('.') {
        segments += 1;
        if segment.is_empty()
            || !segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return false;
        }
    }
    segments == 3
}

fn determine_hint(key: Option<&str>, is_jwt: bool) -> &'static str {
    if is_jwt {
        return "jwt";
    }
    const HINTS: &[(&str, &str)] = &[
        ("password", "password"),
        ("passwd", "password"),
        ("jwt", "jwt"),
        ("cookie", "cookie"),
        ("session", "session"),
        ("credential", "credential"),
        ("auth", "auth"),
        ("token", "token"),
        ("signature", "signature"),
        ("refresh", "refresh"),
    ];
    key.and_then(|k| {
        HINTS
            .iter()
            .find(|(fragment, _)| contains_ignore_ascii_case(k, fragment))
            .map(|(_, hint)| *hint)
    })
    .unwrap_or("secret")
}

/// Redacts sensitive strings in-place, replacing each with
/// `{"$secret": {"id": id, "len": len, "hint": hint}}`.
///
/// Returns `true` if at least one secret was redacted.
pub fn redact_value(val: &mut Value, vault: &SecretVault) -> bool {
    redact_recursive(val, None, false, vault)
}

fn redact_recursive(
    val: &mut Value,
    parent_key: Option<&str>,
    parent_sensitive: bool,
    vault: &SecretVault,
) -> bool {
    match val {
        Value::Object(map) => {
            // Already a redacted secret marker.
            if map.len() == 1 && map.contains_key("$secret") {
                return false;
            }
            let mut redacted = false;
            for (key, child) in map.iter_mut() {
                if matches!(child, Value::Null | Value::Bool(_) | Value::Number(_))
                    || is_exempt_key(key)
                {
                    continue;
                }
                redacted |= redact_recursive(child, Some(key), is_sensitive_key(key), vault);
            }
            redacted
        }
        Value::Array(arr) => {
            let mut redacted = false;
            for child in arr.iter_mut() {
                redacted |= redact_recursive(child, parent_key, parent_sensitive, vault);
            }
            redacted
        }
        Value::String(s) => {
            if s.is_empty() {
                return false;
            }
            let jwt = is_jwt(s);
            if !jwt && !parent_sensitive {
                return false;
            }
            let hint = determine_hint(parent_key, jwt);
            let secret = vault.store(std::mem::take(s), hint.to_string());
            *val = serde_json::json!({
                "$secret": { "id": secret.id, "len": secret.len, "hint": secret.hint }
            });
            true
        }
        _ => false,
    }
}

/// Serialized size of `val` in bytes, computed without allocating the serialization.
pub fn json_len(val: &Value) -> u32 {
    struct Counter(usize);
    impl io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    let _ = serde_json::to_writer(&mut counter, val);
    counter.0.min(u32::MAX as usize) as u32
}

/// Generates a single-line summary (<= `max_len` characters) of a JSON value.
///
/// Serialization stops as soon as enough bytes are available, so previewing a large payload
/// costs the same as previewing a small one.
pub fn generate_preview(val: &Value, max_len: usize) -> String {
    /// Collects at most `limit` bytes then aborts the serializer.
    struct Bounded {
        buf: Vec<u8>,
        limit: usize,
    }
    impl io::Write for Bounded {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let room = self.limit - self.buf.len();
            if room == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            let take = room.min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            Ok(take)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let raw = match val {
        Value::String(s) => s.clone(),
        other => {
            // Worst case 4 bytes per char, plus slack so trimming cannot starve the preview.
            let mut out = Bounded {
                buf: Vec::new(),
                limit: max_len * 4 + 64,
            };
            let _ = serde_json::to_writer(&mut out, other);
            String::from_utf8_lossy(&out.buf).into_owned()
        }
    };

    let mut chars = raw
        .trim()
        .chars()
        .filter(|c| *c != '\r')
        .map(|c| if c == '\n' { ' ' } else { c });
    let mut preview: String = chars.by_ref().take(max_len).collect();
    if chars.next().is_some() && max_len > 0 {
        preview.pop();
        preview.push('…');
    }
    preview
}

/// Clones a JSON value, replacing every `{"$secret": {"id": ...}}` marker with its plaintext
/// secret when the vault still holds it.
pub fn reveal_value(val: &Value, vault: &SecretVault) -> Value {
    match val {
        Value::Object(map) => {
            if map.len() == 1 {
                let revealed = map
                    .get("$secret")
                    .and_then(|meta| meta.get("id"))
                    .and_then(Value::as_u64)
                    .and_then(|id| vault.reveal(id));
                if let Some(secret) = revealed {
                    return Value::String(secret);
                }
            }
            Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), reveal_value(v, vault)))
                    .collect(),
            )
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| reveal_value(v, vault)).collect()),
        other => other.clone(),
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
