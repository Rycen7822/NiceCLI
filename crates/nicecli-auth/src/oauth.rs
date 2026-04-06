use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use thiserror::Error;
use url::Url;

pub const DEFAULT_OAUTH_SESSION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_OAUTH_STATE_LENGTH: usize = 128;

#[derive(Debug, Error)]
pub enum OAuthFlowError {
    #[error("invalid oauth state: {0}")]
    InvalidState(String),
    #[error("unsupported oauth provider")]
    UnsupportedProvider,
    #[error("invalid redirect_url")]
    InvalidRedirectUrl,
    #[error("state is required")]
    MissingState,
    #[error("code or error is required")]
    MissingCodeOrError,
    #[error("oauth flow is not pending")]
    SessionNotPending,
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to encode oauth callback payload: {0}")]
    EncodeCallback(serde_json::Error),
    #[error("failed to write oauth callback file: {0}")]
    WriteCallbackFile(std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSessionSnapshot {
    pub provider: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCallbackInput {
    pub provider: String,
    pub state: String,
    pub code: String,
    pub error: String,
}

#[derive(Debug)]
struct OAuthSession {
    provider: String,
    status: String,
    expires_at: Instant,
}

#[derive(Debug)]
pub struct OAuthSessionStore {
    ttl: Duration,
    sessions: Mutex<HashMap<String, OAuthSession>>,
}

impl Default for OAuthSessionStore {
    fn default() -> Self {
        Self::new(DEFAULT_OAUTH_SESSION_TTL)
    }
}

impl OAuthSessionStore {
    pub fn new(ttl: Duration) -> Self {
        let ttl = if ttl.is_zero() {
            DEFAULT_OAUTH_SESSION_TTL
        } else {
            ttl
        };
        Self {
            ttl,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, state: &str, provider: &str) -> Result<(), OAuthFlowError> {
        let state = validate_oauth_state(state)?;
        let provider = normalize_oauth_provider(provider)?;
        let now = Instant::now();
        let mut sessions = self.lock_sessions();
        Self::purge_expired_locked(&mut sessions, now);
        sessions.insert(
            state,
            OAuthSession {
                provider,
                status: String::new(),
                expires_at: now + self.ttl,
            },
        );
        Ok(())
    }

    pub fn set_error(&self, state: &str, message: &str) -> Result<(), OAuthFlowError> {
        let state = validate_oauth_state(state)?;
        let message = trimmed_or_default(message, "Authentication failed");
        let now = Instant::now();
        let mut sessions = self.lock_sessions();
        Self::purge_expired_locked(&mut sessions, now);
        if let Some(session) = sessions.get_mut(&state) {
            session.status = message;
            session.expires_at = now + self.ttl;
        }
        Ok(())
    }

    pub fn complete(&self, state: &str) -> Result<(), OAuthFlowError> {
        let state = validate_oauth_state(state)?;
        let now = Instant::now();
        let mut sessions = self.lock_sessions();
        Self::purge_expired_locked(&mut sessions, now);
        sessions.remove(&state);
        Ok(())
    }

    pub fn complete_provider(&self, provider: &str) -> Result<usize, OAuthFlowError> {
        let provider = normalize_oauth_provider(provider)?;
        let now = Instant::now();
        let mut sessions = self.lock_sessions();
        Self::purge_expired_locked(&mut sessions, now);

        let matching_states: Vec<String> = sessions
            .iter()
            .filter_map(|(state, session)| {
                if session.provider.eq_ignore_ascii_case(&provider) {
                    Some(state.clone())
                } else {
                    None
                }
            })
            .collect();

        let removed = matching_states.len();
        for state in matching_states {
            sessions.remove(&state);
        }
        Ok(removed)
    }

    pub fn get(&self, state: &str) -> Result<Option<OAuthSessionSnapshot>, OAuthFlowError> {
        let state = validate_oauth_state(state)?;
        let now = Instant::now();
        let mut sessions = self.lock_sessions();
        Self::purge_expired_locked(&mut sessions, now);

        Ok(sessions.get(&state).map(|session| OAuthSessionSnapshot {
            provider: session.provider.clone(),
            status: session.status.clone(),
        }))
    }

    pub fn is_pending(&self, state: &str, provider: Option<&str>) -> Result<bool, OAuthFlowError> {
        let session = match self.get(state)? {
            Some(session) => session,
            None => return Ok(false),
        };

        if !session.status.is_empty() {
            return Ok(false);
        }

        let Some(provider) = provider else {
            return Ok(true);
        };

        let provider = normalize_oauth_provider(provider)?;
        Ok(session.provider.eq_ignore_ascii_case(&provider))
    }

    fn lock_sessions(&self) -> MutexGuard<'_, HashMap<String, OAuthSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn purge_expired_locked(sessions: &mut HashMap<String, OAuthSession>, now: Instant) {
        sessions.retain(|_, session| session.expires_at > now);
    }
}

pub fn validate_oauth_state(state: &str) -> Result<String, OAuthFlowError> {
    let trimmed = state.trim();
    if trimmed.is_empty() {
        return Err(OAuthFlowError::InvalidState("empty".to_string()));
    }
    if trimmed.len() > MAX_OAUTH_STATE_LENGTH {
        return Err(OAuthFlowError::InvalidState("too long".to_string()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(OAuthFlowError::InvalidState(
            "contains path separator".to_string(),
        ));
    }
    if trimmed.contains("..") {
        return Err(OAuthFlowError::InvalidState("contains '..'".to_string()));
    }

    for character in trimmed.chars() {
        match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => {}
            _ => {
                return Err(OAuthFlowError::InvalidState(
                    "invalid character".to_string(),
                ))
            }
        }
    }

    Ok(trimmed.to_string())
}

pub fn normalize_oauth_provider(provider: &str) -> Result<String, OAuthFlowError> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => Ok("anthropic".to_string()),
        "codex" | "openai" => Ok("codex".to_string()),
        "gemini" | "google" => Ok("gemini".to_string()),
        "antigravity" | "anti-gravity" => Ok("antigravity".to_string()),
        "qwen" => Ok("qwen".to_string()),
        "kimi" => Ok("kimi".to_string()),
        _ => Err(OAuthFlowError::UnsupportedProvider),
    }
}

pub fn resolve_oauth_callback_input(
    provider: &str,
    redirect_url: Option<&str>,
    state: Option<&str>,
    code: Option<&str>,
    error_message: Option<&str>,
) -> Result<OAuthCallbackInput, OAuthFlowError> {
    let provider = normalize_oauth_provider(provider)?;
    let mut resolved_state = trim_optional(state).unwrap_or_default();
    let mut resolved_code = trim_optional(code).unwrap_or_default();
    let mut resolved_error = trim_optional(error_message).unwrap_or_default();

    if let Some(redirect_url) = trim_optional(redirect_url) {
        let url = Url::parse(&redirect_url).map_err(|_| OAuthFlowError::InvalidRedirectUrl)?;
        for (key, value) in url.query_pairs() {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.as_ref() {
                "state" if resolved_state.is_empty() => resolved_state = value.to_string(),
                "code" if resolved_code.is_empty() => resolved_code = value.to_string(),
                "error" if resolved_error.is_empty() => resolved_error = value.to_string(),
                "error_description" if resolved_error.is_empty() => {
                    resolved_error = value.to_string()
                }
                _ => {}
            }
        }
    }

    if resolved_state.is_empty() {
        return Err(OAuthFlowError::MissingState);
    }
    let state = validate_oauth_state(&resolved_state)?;

    if resolved_code.is_empty() && resolved_error.is_empty() {
        return Err(OAuthFlowError::MissingCodeOrError);
    }

    Ok(OAuthCallbackInput {
        provider,
        state,
        code: resolved_code,
        error: resolved_error,
    })
}

pub fn write_oauth_callback_file(
    auth_dir: &Path,
    provider: &str,
    state: &str,
    code: &str,
    error_message: &str,
) -> Result<PathBuf, OAuthFlowError> {
    let provider = normalize_oauth_provider(provider)?;
    let state = validate_oauth_state(state)?;

    fs::create_dir_all(auth_dir).map_err(OAuthFlowError::CreateAuthDir)?;

    let payload = OAuthCallbackFilePayload {
        code: trim_optional(Some(code)).unwrap_or_default(),
        state: state.clone(),
        error: trim_optional(Some(error_message)).unwrap_or_default(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(OAuthFlowError::EncodeCallback)?;

    let path = auth_dir.join(format!(".oauth-{provider}-{state}.oauth"));
    fs::write(&path, bytes).map_err(OAuthFlowError::WriteCallbackFile)?;
    Ok(path)
}

pub fn write_oauth_callback_file_for_pending_session(
    auth_dir: &Path,
    sessions: &OAuthSessionStore,
    provider: &str,
    state: &str,
    code: &str,
    error_message: &str,
) -> Result<PathBuf, OAuthFlowError> {
    let provider = normalize_oauth_provider(provider)?;
    if !sessions.is_pending(state, Some(&provider))? {
        return Err(OAuthFlowError::SessionNotPending);
    }
    write_oauth_callback_file(auth_dir, &provider, state, code, error_message)
}

fn trim_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn trimmed_or_default(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Serialize)]
struct OAuthCallbackFilePayload {
    code: String,
    state: String,
    error: String,
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_oauth_callback_input, write_oauth_callback_file_for_pending_session,
        OAuthSessionStore,
    };
    use serde_json::Value;
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn oauth_session_store_tracks_pending_and_error_states() {
        let sessions = OAuthSessionStore::new(Duration::from_secs(60));
        sessions
            .register("codex-state_01", "openai")
            .expect("register session");

        let snapshot = sessions
            .get("codex-state_01")
            .expect("get session")
            .expect("session");
        assert_eq!(snapshot.provider, "codex");
        assert!(snapshot.status.is_empty());
        assert!(sessions
            .is_pending("codex-state_01", Some("codex"))
            .expect("pending"));

        sessions
            .set_error("codex-state_01", " Timeout waiting ")
            .expect("set error");
        let snapshot = sessions
            .get("codex-state_01")
            .expect("get session")
            .expect("session");
        assert_eq!(snapshot.status, "Timeout waiting");
        assert!(!sessions
            .is_pending("codex-state_01", Some("codex"))
            .expect("not pending"));

        sessions
            .complete("codex-state_01")
            .expect("complete session");
        assert!(sessions.get("codex-state_01").expect("get").is_none());
    }

    #[test]
    fn resolves_callback_from_redirect_url_and_writes_pending_file() {
        let temp_dir = TempDir::new().expect("temp dir");
        let sessions = OAuthSessionStore::new(Duration::from_secs(60));
        sessions
            .register("codex-callback-01", "codex")
            .expect("register session");

        let callback = resolve_oauth_callback_input(
            "openai",
            Some("http://127.0.0.1:1455/codex/callback?state=codex-callback-01&code=auth-code"),
            None,
            None,
            None,
        )
        .expect("resolve callback");

        let path = write_oauth_callback_file_for_pending_session(
            temp_dir.path(),
            &sessions,
            &callback.provider,
            &callback.state,
            &callback.code,
            &callback.error,
        )
        .expect("write callback file");

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some(".oauth-codex-codex-callback-01.oauth")
        );

        let payload: Value = serde_json::from_str(
            &fs::read_to_string(&path).expect("callback payload should exist"),
        )
        .expect("json");
        assert_eq!(payload["state"].as_str(), Some("codex-callback-01"));
        assert_eq!(payload["code"].as_str(), Some("auth-code"));
        assert_eq!(payload["error"].as_str(), Some(""));
    }
}
