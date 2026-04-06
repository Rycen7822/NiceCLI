use crate::{write_auth_file, AuthFileStoreError};
use serde_json::json;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGeminiWebTokens {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
}

#[derive(Debug, Error)]
pub enum GeminiWebTokenError {
    #[error("email is required")]
    MissingEmail,
    #[error("Secure-1PSID is required")]
    MissingSecure1Psid,
    #[error("Secure-1PSIDTS is required")]
    MissingSecure1PsidTs,
    #[error("failed to encode auth file: {0}")]
    Encode(serde_json::Error),
    #[error(transparent)]
    Write(#[from] AuthFileStoreError),
}

pub fn save_gemini_web_tokens(
    auth_dir: &Path,
    email: &str,
    secure_1psid: &str,
    secure_1psidts: &str,
) -> Result<SavedGeminiWebTokens, GeminiWebTokenError> {
    let email = trim_required(email).ok_or(GeminiWebTokenError::MissingEmail)?;
    let secure_1psid =
        trim_required(secure_1psid).ok_or(GeminiWebTokenError::MissingSecure1Psid)?;
    let secure_1psidts =
        trim_required(secure_1psidts).ok_or(GeminiWebTokenError::MissingSecure1PsidTs)?;

    let file_name = format!("gemini-web-{email}.json");
    let cookie = format!("Secure-1PSID={secure_1psid}; Secure-1PSIDTS={secure_1psidts};");
    let payload = json!({
        "id": file_name,
        "provider": "gemini",
        "type": "gemini",
        "auth_mode": "web_cookies",
        "email": email,
        "label": email,
        "cookie": cookie,
        "cookies": {
            "Secure-1PSID": secure_1psid,
            "Secure-1PSIDTS": secure_1psidts,
        },
        "secure_1psid": secure_1psid,
        "secure_1psidts": secure_1psidts,
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(GeminiWebTokenError::Encode)?;
    let written_name = write_auth_file(auth_dir, &file_name, &bytes)?;

    Ok(SavedGeminiWebTokens {
        file_path: auth_dir.join(&written_name),
        file_name: written_name,
        email: email.to_string(),
    })
}

fn trim_required(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
