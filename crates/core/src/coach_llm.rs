//! Opt-in Layer 2 Coach LLM Narrator (`Settings::coach_llm`).
//!
//! Turns a verified, identity-free [`GameReview`] from Layer 1 into a short narrative coach
//! debrief via a local OpenAI-compatible server (Ollama or LM Studio on `127.0.0.1`) or a
//! player-provided BYOK HTTPS API endpoint.
//!
//! Invariants:
//! - Switched off (`!flags.coach_llm()`), performs zero network I/O.
//! - Prompts are built exclusively from [`GameReview`] via [`build_coach_prompt`]: no summoner
//!   name, PUUID, or lockfile secret can ever leave the machine.
//! - Plain HTTP is only permitted for loopback (`127.0.0.1`, `localhost`, `[::1]`); any remote
//!   endpoint must use `https://`.
//! - The optional API key is masked in `Debug` output and never logged.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use hypatia_shared::{
    build_coach_prompt_with_focus, build_followup_messages, demo_coach_debrief_for_focus,
    demo_coach_followup, CoachFocus, CoachMessage, CoachTurn, GameReview, Lang,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::discovery::DiscoveryManager;
use crate::settings::{default_config_dir, Flags};
use crate::signal::ChangeSignal;

const DEBRIEF_TIMEOUT: Duration = Duration::from_secs(45);
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Built-in endpoint presets for the opt-in LLM narrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachLlmPreset {
    #[default]
    OllamaLocal,
    LmStudioLocal,
    Custom,
}

impl CoachLlmPreset {
    pub fn as_index(self) -> i32 {
        match self {
            Self::OllamaLocal => 0,
            Self::LmStudioLocal => 1,
            Self::Custom => 2,
        }
    }

    pub fn from_index(idx: i32) -> Self {
        match idx {
            0 => Self::OllamaLocal,
            1 => Self::LmStudioLocal,
            _ => Self::Custom,
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::OllamaLocal => "http://127.0.0.1:11434/v1",
            Self::LmStudioLocal => "http://127.0.0.1:1234/v1",
            Self::Custom => "https://api.openai.com/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::OllamaLocal => "llama3.1:8b",
            Self::LmStudioLocal => "local-model",
            Self::Custom => "gpt-4o-mini",
        }
    }
}

/// Persisted connection config for the opt-in LLM narrator (`coach_llm.json`).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoachLlmConfig {
    pub preset: CoachLlmPreset,
    pub base_url: String,
    pub model: String,
    /// Optional Bearer API key (empty for local Ollama / LM Studio). Never logged.
    pub api_key: String,
}

impl Default for CoachLlmConfig {
    fn default() -> Self {
        Self::for_preset(CoachLlmPreset::OllamaLocal)
    }
}

impl CoachLlmConfig {
    pub fn for_preset(preset: CoachLlmPreset) -> Self {
        Self {
            preset,
            base_url: preset.default_base_url().to_string(),
            model: preset.default_model().to_string(),
            api_key: String::new(),
        }
    }

    /// Validates `base_url`: loopback may use `http://` or `https://`; remote hosts require `https://`.
    pub fn validate_base_url(&self, lang: Lang) -> Result<String, String> {
        let trimmed = self.base_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(lang
                .pick("Endpoint URL is empty.", "L'URL du serveur est vide.")
                .into());
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("https://") {
            return Ok(trimmed.to_string());
        }
        if let Some(rest) = lower.strip_prefix("http://") {
            let host = rest.split('/').next().unwrap_or("");
            let is_loopback = host == "127.0.0.1"
                || host.starts_with("127.0.0.1:")
                || host == "localhost"
                || host.starts_with("localhost:")
                || host == "[::1]"
                || host.starts_with("[::1]:");
            if is_loopback {
                return Ok(trimmed.to_string());
            }
            return Err(lang
                .pick(
                    "Remote endpoints must use https:// (plain http:// is only allowed on 127.0.0.1 / localhost).",
                    "Les serveurs distants doivent utiliser https:// (http:// n'est autorisé que sur 127.0.0.1 / localhost).",
                )
                .into());
        }
        Err(lang
            .pick(
                "Endpoint URL must start with http://127.0.0.1 or https://.",
                "L'URL doit commencer par http://127.0.0.1 ou https://.",
            )
            .into())
    }
}

impl fmt::Debug for CoachLlmConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoachLlmConfig")
            .field("preset", &self.preset)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    ""
                } else {
                    "[REDACTED]"
                },
            )
            .finish()
    }
}

/// Current state of a coach debrief + follow-up Q&A session for one `(game_id, lang)`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DebriefState {
    #[default]
    Idle,
    Loading(CoachFocus),
    Ready {
        text: String,
        model: String,
        is_demo: bool,
        focus: CoachFocus,
        turns: Vec<CoachTurn>,
        followup_loading: bool,
        followup_error: Option<String>,
    },
    Failed {
        message: String,
        focus: CoachFocus,
    },
}

/// Current state of the Performance sheet's "Test connection" action.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionTestState {
    #[default]
    Idle,
    Testing,
    Ok(String),
    Failed(String),
}

/// Non-blocking service mounted on `AppState::coach_llm`.
#[derive(Clone)]
pub struct CoachDebriefService {
    flags: Arc<Flags>,
    discovery: Arc<DiscoveryManager>,
    signal: ChangeSignal,
    http: Client,
    config: Arc<RwLock<CoachLlmConfig>>,
    config_path: Option<PathBuf>,
    debriefs: Arc<RwLock<HashMap<(i64, u8), DebriefState>>>,
    conn_test: Arc<RwLock<ConnectionTestState>>,
    revision: Arc<AtomicU64>,
}

impl CoachDebriefService {
    pub fn new(
        flags: Arc<Flags>,
        discovery: Arc<DiscoveryManager>,
        signal: ChangeSignal,
        config_path: Option<PathBuf>,
    ) -> Self {
        let loaded = config_path
            .as_deref()
            .map(load_config_from)
            .unwrap_or_default();
        let http = Client::builder()
            .user_agent("hypatia-coach/1")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            flags,
            discovery,
            signal,
            http,
            config: Arc::new(RwLock::new(loaded)),
            config_path,
            debriefs: Arc::new(RwLock::new(HashMap::new())),
            conn_test: Arc::new(RwLock::new(ConnectionTestState::Idle)),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn default_config_path() -> PathBuf {
        default_config_dir().join("coach_llm.json")
    }

    /// Monotonic revision bumped whenever config, debrief state, or connection test changes.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    fn bump(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
        self.signal.bump();
    }

    /// True when the coach debrief action should be offered in the UI (`coach_llm` on or demo).
    pub fn is_available(&self) -> bool {
        self.flags.coach_llm() || self.discovery.is_demo_override()
    }

    pub fn config(&self) -> CoachLlmConfig {
        self.config.read().unwrap().clone()
    }

    pub fn save_config(&self, cfg: CoachLlmConfig) {
        *self.config.write().unwrap() = cfg.clone();
        *self.conn_test.write().unwrap() = ConnectionTestState::Idle;
        if let Some(path) = &self.config_path {
            save_config_to(path, &cfg);
        }
        self.bump();
    }

    pub fn state(&self, game_id: i64, lang: Lang) -> DebriefState {
        self.debriefs
            .read()
            .unwrap()
            .get(&(game_id, lang as u8))
            .cloned()
            .unwrap_or(DebriefState::Idle)
    }

    pub fn connection_test_state(&self) -> ConnectionTestState {
        self.conn_test.read().unwrap().clone()
    }

    /// Requests a Layer 2 coach debrief for `review` using the default [`CoachFocus::Overview`].
    pub fn request_debrief(&self, review: GameReview, lang: Lang) {
        self.request_debrief_with_focus(review, CoachFocus::Overview, lang);
    }

    /// Requests a Layer 2 coach debrief for `review` under a specific [`CoachFocus`] lens.
    ///
    /// Performs zero network I/O when `flags.coach_llm()` is off (outside demo mode).
    pub fn request_debrief_with_focus(&self, review: GameReview, focus: CoachFocus, lang: Lang) {
        let key = (review.game_id, lang as u8);
        if self.discovery.is_demo_override() {
            let text = demo_coach_debrief_for_focus(&review, focus, lang);
            self.debriefs.write().unwrap().insert(
                key,
                DebriefState::Ready {
                    text,
                    model: lang.pick("Demo", "Démo").to_string(),
                    is_demo: true,
                    focus,
                    turns: Vec::new(),
                    followup_loading: false,
                    followup_error: None,
                },
            );
            self.bump();
            return;
        }
        if !self.flags.coach_llm() {
            return;
        }
        {
            let mut map = self.debriefs.write().unwrap();
            if matches!(map.get(&key), Some(DebriefState::Loading(_))) {
                return;
            }
            map.insert(key, DebriefState::Loading(focus));
        }
        self.bump();

        let cfg = self.config();
        let http = self.http.clone();
        let debriefs = self.debriefs.clone();
        let signal = self.signal.clone();
        let revision = self.revision.clone();

        tokio::spawn(async move {
            let outcome = generate_debrief_with_focus(&http, &cfg, &review, focus, lang).await;
            let next = match outcome {
                Ok((text, model)) => DebriefState::Ready {
                    text,
                    model,
                    is_demo: false,
                    focus,
                    turns: Vec::new(),
                    followup_loading: false,
                    followup_error: None,
                },
                Err(message) => DebriefState::Failed { message, focus },
            };
            debriefs.write().unwrap().insert(key, next);
            revision.fetch_add(1, Ordering::Relaxed);
            signal.bump();
        });
    }

    /// Asks a grounded follow-up question in the active match's coaching session.
    ///
    /// Performs zero network I/O when `flags.coach_llm()` is off (outside demo mode).
    pub fn ask_followup(&self, review: GameReview, question: String, lang: Lang) {
        let q = question.trim().to_string();
        if q.is_empty() {
            return;
        }
        let key = (review.game_id, lang as u8);

        if self.discovery.is_demo_override() {
            let answer = demo_coach_followup(&review, &q, lang);
            let mut map = self.debriefs.write().unwrap();
            let entry = map.entry(key).or_insert_with(|| DebriefState::Ready {
                text: demo_coach_debrief_for_focus(&review, CoachFocus::Overview, lang),
                model: lang.pick("Demo", "Démo").to_string(),
                is_demo: true,
                focus: CoachFocus::Overview,
                turns: Vec::new(),
                followup_loading: false,
                followup_error: None,
            });
            if let DebriefState::Ready {
                turns,
                followup_loading,
                followup_error,
                ..
            } = entry
            {
                turns.push(CoachTurn {
                    question: q,
                    answer,
                });
                if turns.len() > 8 {
                    let excess = turns.len() - 8;
                    turns.drain(0..excess);
                }
                *followup_loading = false;
                *followup_error = None;
            }
            drop(map);
            self.bump();
            return;
        }

        if !self.flags.coach_llm() {
            return;
        }

        let (focus, initial_debrief, history) = {
            let mut map = self.debriefs.write().unwrap();
            match map.get_mut(&key) {
                Some(DebriefState::Ready {
                    text,
                    focus,
                    turns,
                    followup_loading,
                    followup_error,
                    ..
                }) => {
                    if *followup_loading {
                        return;
                    }
                    *followup_loading = true;
                    *followup_error = None;
                    (*focus, text.clone(), turns.clone())
                }
                _ => return,
            }
        };
        self.bump();

        let cfg = self.config();
        let http = self.http.clone();
        let debriefs = self.debriefs.clone();
        let signal = self.signal.clone();
        let revision = self.revision.clone();

        tokio::spawn(async move {
            let outcome = generate_followup(
                &http,
                &cfg,
                &review,
                focus,
                &initial_debrief,
                &history,
                &q,
                lang,
            )
            .await;
            {
                let mut map = debriefs.write().unwrap();
                if let Some(DebriefState::Ready {
                    turns,
                    followup_loading,
                    followup_error,
                    ..
                }) = map.get_mut(&key)
                {
                    *followup_loading = false;
                    match outcome {
                        Ok((answer, _)) => {
                            turns.push(CoachTurn {
                                question: q,
                                answer,
                            });
                            if turns.len() > 8 {
                                let excess = turns.len() - 8;
                                turns.drain(0..excess);
                            }
                            *followup_error = None;
                        }
                        Err(err) => {
                            *followup_error = Some(err);
                        }
                    }
                }
            }
            revision.fetch_add(1, Ordering::Relaxed);
            signal.bump();
        });
    }

    /// Probes `{base_url}/models` to verify the local/BYOK LLM endpoint is reachable.
    pub fn test_connection(&self, lang: Lang) {
        if !self.flags.coach_llm() {
            return;
        }
        *self.conn_test.write().unwrap() = ConnectionTestState::Testing;
        self.bump();

        let cfg = self.config();
        let http = self.http.clone();
        let conn_test = self.conn_test.clone();
        let signal = self.signal.clone();
        let revision = self.revision.clone();

        tokio::spawn(async move {
            let next = match probe_models(&http, &cfg, lang).await {
                Ok(msg) => ConnectionTestState::Ok(msg),
                Err(msg) => ConnectionTestState::Failed(msg),
            };
            *conn_test.write().unwrap() = next;
            revision.fetch_add(1, Ordering::Relaxed);
            signal.bump();
        });
    }
}

/// Calls an OpenAI-compatible `POST {base_url}/chat/completions` endpoint with anonymized facts
/// using the default [`CoachFocus::Overview`] lens.
pub async fn generate_debrief(
    http: &Client,
    cfg: &CoachLlmConfig,
    review: &GameReview,
    lang: Lang,
) -> Result<(String, String), String> {
    generate_debrief_with_focus(http, cfg, review, CoachFocus::Overview, lang).await
}

/// Calls an OpenAI-compatible `POST {base_url}/chat/completions` endpoint for a specific [`CoachFocus`].
pub async fn generate_debrief_with_focus(
    http: &Client,
    cfg: &CoachLlmConfig,
    review: &GameReview,
    focus: CoachFocus,
    lang: Lang,
) -> Result<(String, String), String> {
    let prompt = build_coach_prompt_with_focus(review, focus, lang);
    let messages = vec![
        CoachMessage {
            role: "system",
            content: prompt.system,
        },
        CoachMessage {
            role: "user",
            content: prompt.user,
        },
    ];
    call_chat_completions(http, cfg, &messages, 520, lang).await
}

/// Sends a grounded multi-turn follow-up question to the OpenAI-compatible endpoint.
#[allow(clippy::too_many_arguments)]
pub async fn generate_followup(
    http: &Client,
    cfg: &CoachLlmConfig,
    review: &GameReview,
    focus: CoachFocus,
    initial_debrief: &str,
    history: &[CoachTurn],
    question: &str,
    lang: Lang,
) -> Result<(String, String), String> {
    let messages = build_followup_messages(review, focus, initial_debrief, history, question, lang);
    call_chat_completions(http, cfg, &messages, 320, lang).await
}

async fn call_chat_completions(
    http: &Client,
    cfg: &CoachLlmConfig,
    messages: &[CoachMessage],
    max_tokens: u32,
    lang: Lang,
) -> Result<(String, String), String> {
    let base = cfg.validate_base_url(lang)?;
    let model = cfg.model.trim();
    if model.is_empty() {
        return Err(lang
            .pick("Model name is empty.", "Le nom du modèle est vide.")
            .into());
    }
    let url = format!("{base}/chat/completions");
    let wire_messages: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();
    let body = json!({
        "model": model,
        "temperature": 0.35,
        "max_tokens": max_tokens,
        "messages": wire_messages,
    });

    let mut req = http.post(&url).timeout(DEBRIEF_TIMEOUT).json(&body);
    let key = cfg.api_key.trim();
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            lang.pick(
                "The LLM endpoint timed out. Try a smaller model or check the server.",
                "Délai dépassé pour le modèle LLM. Essayez un modèle plus léger ou vérifiez le serveur.",
            )
            .to_string()
        } else {
            lang.pick(
                "Could not reach the LLM endpoint. Is Ollama / LM Studio running?",
                "Impossible de joindre le serveur LLM. Ollama / LM Studio est-il lancé ?",
            )
            .to_string()
        }
    })?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(lang
            .pick(
                "Authentication failed (HTTP 401/403). Check your API key.",
                "Authentification refusée (HTTP 401/403). Vérifiez votre clé API.",
            )
            .into());
    }
    if status.as_u16() == 404 {
        return Err(lang
            .pick(
                "Model or endpoint not found (HTTP 404). Check the URL and model name.",
                "Modèle ou endpoint introuvable (HTTP 404). Vérifiez l'URL et le nom du modèle.",
            )
            .into());
    }
    if !status.is_success() {
        return Err(format!(
            "{} (HTTP {})",
            lang.pick(
                "The LLM server returned an error",
                "Le serveur LLM a renvoyé une erreur"
            ),
            status.as_u16()
        ));
    }

    let val: Value = resp.json().await.map_err(|_| {
        lang.pick(
            "Unexpected response format from the LLM server.",
            "Format de réponse inattendu du serveur LLM.",
        )
        .to_string()
    })?;
    let raw_content = val
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            lang.pick(
                "The LLM returned an empty debrief.",
                "Le modèle LLM a renvoyé une réponse vide.",
            )
            .to_string()
        })?;
    let content = sanitize_debrief(raw_content);
    if content.is_empty() {
        return Err(lang
            .pick(
                "The LLM returned an empty debrief.",
                "Le modèle LLM a renvoyé une réponse vide.",
            )
            .to_string());
    }
    let used_model = val
        .get("model")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(model)
        .to_string();
    Ok((content, used_model))
}

/// Cleans an LLM response for plain-text display in Slint:
/// - Strips `<think>...</think>` blocks emitted by local reasoning models (DeepSeek-R1, Qwen3, QwQ).
/// - Strips accidental Markdown (`**`, `__`, leading `# ` headings) and normalizes `- ` / `* ` bullets to `• `.
/// - Collapses excessive blank lines.
pub fn sanitize_debrief(raw: &str) -> String {
    // Remove any closed <think>...</think> reasoning sections.
    let mut without_think = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("<think>") {
        without_think.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</think>") {
            rest = &rest[start + end + "</think>".len()..];
        } else {
            rest = "";
            break;
        }
    }
    without_think.push_str(rest);

    let mut lines: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for line in without_think.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank && !lines.is_empty() {
                lines.push(String::new());
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;
        let stripped_heading = trimmed.trim_start_matches('#').trim_start();
        let cleaned = stripped_heading
            .replace("**", "")
            .replace("__", "")
            .replace('`', "");
        let normalized = if let Some(body) = cleaned
            .strip_prefix("- ")
            .or_else(|| cleaned.strip_prefix("* "))
        {
            format!("• {}", body.trim_start())
        } else {
            cleaned
        };
        lines.push(normalized);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

async fn probe_models(http: &Client, cfg: &CoachLlmConfig, lang: Lang) -> Result<String, String> {
    let base = cfg.validate_base_url(lang)?;
    let url = format!("{base}/models");
    let mut req = http.get(&url).timeout(TEST_TIMEOUT);
    let key = cfg.api_key.trim();
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|_| {
        lang.pick(
            "Endpoint unreachable. Start Ollama / LM Studio or check the URL.",
            "Serveur injoignable. Lancez Ollama / LM Studio ou vérifiez l'URL.",
        )
        .to_string()
    })?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(lang
            .pick(
                "Authentication failed (HTTP 401/403). Check your API key.",
                "Authentification refusée (HTTP 401/403). Vérifiez votre clé API.",
            )
            .into());
    }
    if !status.is_success() {
        return Err(format!(
            "{} (HTTP {})",
            lang.pick(
                "Server answered with an error",
                "Le serveur a répondu par une erreur"
            ),
            status.as_u16()
        ));
    }
    Ok(lang
        .pick("Connected and ready.", "Connecté et prêt.")
        .to_string())
}

fn load_config_from(path: &Path) -> CoachLlmConfig {
    let Ok(bytes) = std::fs::read(path) else {
        return CoachLlmConfig::default();
    };
    match serde_json::from_slice::<CoachLlmConfig>(&bytes) {
        Ok(cfg) => cfg,
        Err(e) => {
            warn!("Could not parse {}: {e}", path.display());
            CoachLlmConfig::default()
        }
    }
}

fn save_config_to(path: &Path, cfg: &CoachLlmConfig) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("Could not create {}: {e}", parent.display());
            return;
        }
    }
    let Ok(json) = serde_json::to_vec_pretty(cfg) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &json).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::SecretVault;
    use crate::settings::Settings;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use hypatia_shared::{analyze_match, Position, ReviewInput};
    use tokio::net::TcpListener;

    fn sample_review() -> GameReview {
        let input = ReviewInput {
            game_id: 77,
            champion_id: 103,
            position: Some(Position::Mid),
            win: true,
            duration_sec: 1680,
            kills: 8,
            deaths: 2,
            assists: 9,
            creep_score: 215,
            total_gold: 13200,
            damage_to_champions: 28000,
            vision_score: 28,
            kill_participation: Some(0.68),
            items: vec![3020, 3165, 3089],
            ally_champions: vec![54, 64, 103, 51, 89],
            enemy_champions: vec![266, 19, 8, 222, 16],
            opponent_champion_id: Some(8),
            ..ReviewInput::default()
        };
        analyze_match(&input, Lang::En)
    }

    #[test]
    fn debug_masks_api_key() {
        let cfg = CoachLlmConfig {
            preset: CoachLlmPreset::Custom,
            base_url: "https://api.example.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key: "sk-super-secret-token-123".into(),
        };
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("sk-super-secret-token-123"));
        assert!(dbg.contains("[REDACTED]"));
    }

    #[test]
    fn validates_loopback_http_and_requires_https_for_remote() {
        let local = CoachLlmConfig::for_preset(CoachLlmPreset::OllamaLocal);
        assert!(local.validate_base_url(Lang::En).is_ok());

        let remote_http = CoachLlmConfig {
            base_url: "http://example.com/v1".into(),
            ..local.clone()
        };
        assert!(remote_http.validate_base_url(Lang::En).is_err());

        let remote_https = CoachLlmConfig {
            base_url: "https://api.openai.com/v1/".into(),
            ..local
        };
        assert_eq!(
            remote_https.validate_base_url(Lang::En).unwrap(),
            "https://api.openai.com/v1"
        );
    }

    #[tokio::test]
    async fn switched_off_performs_no_io() {
        let signal = ChangeSignal::new();
        let flags = Arc::new(Flags::new(&Settings::default()));
        assert!(!flags.coach_llm());
        let discovery = DiscoveryManager::new(SecretVault::new(), signal.clone());
        let svc = CoachDebriefService::new(flags, discovery, signal, None);

        svc.request_debrief(sample_review(), Lang::En);
        assert_eq!(svc.state(77, Lang::En), DebriefState::Idle);
    }

    #[tokio::test]
    async fn mock_openai_compat_handles_success_and_auth_error() {
        let app = Router::new()
            .route(
                "/v1/models",
                get(|| async { Json(json!({"data": [{"id": "llama3.1:8b"}]})) }),
            )
            .route(
                "/v1/chat/completions",
                post(|Json(payload): Json<Value>| async move {
                    let model = payload["model"].as_str().unwrap_or("unknown");
                    if model == "bad-auth" {
                        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "bad key"})));
                    }
                    (
                        StatusCode::OK,
                        Json(json!({
                            "model": model,
                            "choices": [{"message": {"content": "Clean mid-game control. Keep resetting before Dragon."}}]
                        })),
                    )
                }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let http = Client::new();
        let cfg = CoachLlmConfig {
            preset: CoachLlmPreset::OllamaLocal,
            base_url: format!("http://{addr}/v1"),
            model: "llama3.1:8b".into(),
            api_key: String::new(),
        };

        let ok_probe = probe_models(&http, &cfg, Lang::En).await;
        assert!(ok_probe.is_ok(), "probe failed: {ok_probe:?}");

        let (text, used_model) = generate_debrief(&http, &cfg, &sample_review(), Lang::En)
            .await
            .expect("debrief should succeed");
        assert!(text.contains("Clean mid-game control"));
        assert_eq!(used_model, "llama3.1:8b");

        let bad_cfg = CoachLlmConfig {
            model: "bad-auth".into(),
            ..cfg
        };
        let err = generate_debrief(&http, &bad_cfg, &sample_review(), Lang::Fr)
            .await
            .unwrap_err();
        assert!(err.contains("401/403"), "got {err}");
    }

    #[test]
    fn sanitize_debrief_strips_think_tags_and_markdown() {
        let raw = "<think>\nChecking CS and death timestamps...\n</think>\n\n### **VERDICT**\nClean lane on **Ahri**.\n\n\n## KEY LEVERS\n- Reset before `Dragon`.\n* Buy **Oblivion Orb** early.\n\n";
        let cleaned = sanitize_debrief(raw);
        assert_eq!(
            cleaned,
            "VERDICT\nClean lane on Ahri.\n\nKEY LEVERS\n• Reset before Dragon.\n• Buy Oblivion Orb early."
        );
    }

    #[tokio::test]
    async fn generate_followup_sends_multi_turn_history() {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|Json(payload): Json<Value>| async move {
                let count = payload["messages"].as_array().map(Vec::len).unwrap_or(0);
                (
                    StatusCode::OK,
                    Json(json!({
                        "model": "llama3.1:8b",
                        "choices": [{"message": {"content": format!("Grounded reply with {count} messages.")}}]
                    })),
                )
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let http = Client::new();
        let cfg = CoachLlmConfig {
            preset: CoachLlmPreset::OllamaLocal,
            base_url: format!("http://{addr}/v1"),
            model: "llama3.1:8b".into(),
            api_key: String::new(),
        };
        let (reply, _) = generate_followup(
            &http,
            &cfg,
            &sample_review(),
            CoachFocus::Matchup,
            "Initial matchup breakdown.",
            &[CoachTurn {
                question: "When do I spike?".into(),
                answer: "At Lost Chapter and level 6.".into(),
            }],
            "Should I buy Oblivion Orb first?",
            Lang::En,
        )
        .await
        .expect("followup should succeed");
        // system + dossier + initial debrief + 1 prior turn (2 msgs) + new question = 6
        assert!(reply.contains("6 messages"), "got {reply}");
    }
}
