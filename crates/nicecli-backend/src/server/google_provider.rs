use super::*;
use nicecli_auth::{
    import_vertex_credential as import_vertex_credential_to_store,
    save_gemini_web_tokens as save_gemini_web_tokens_to_store, GeminiWebTokenError,
    VertexCredentialImportError,
};
use nicecli_runtime::{
    AntigravityCallerError, AntigravityGenerateContentCaller, AntigravityGenerateContentRequest,
    ExecuteWithRetryError,
};

mod api_keys;
mod auth_flows;
mod internal_auth;
mod internal_methods;
mod model_catalog;
mod public_action_auth_files;
mod public_action_requests;
mod public_actions;
mod public_transport;
mod response_helpers;

pub(in crate::server) use self::api_keys::{
    delete_gemini_api_keys, delete_vertex_api_keys, get_gemini_api_keys, get_vertex_api_keys,
    patch_gemini_api_keys, patch_vertex_api_keys, put_gemini_api_keys, put_vertex_api_keys,
};
use self::api_keys::{
    gemini_api_key_entries_from_config_json, resolve_gemini_api_key_entry_model,
    resolve_vertex_api_key_entry_model, vertex_api_key_entries_from_config_json,
};
pub(super) use self::auth_flows::{
    get_gemini_cli_auth_url, handle_google_callback, import_vertex_credential,
    save_gemini_web_tokens,
};
use self::internal_auth::*;
pub(super) use self::internal_methods::handle_v1internal_method;
pub(super) use self::model_catalog::{collect_public_gemini_models, find_public_gemini_model};
pub(super) use self::public_action_requests::parse_gemini_public_action;
pub(super) use self::public_actions::execute_public_gemini_model_action;
use self::public_transport::*;
use self::response_helpers::*;

pub(super) const DEFAULT_GEMINI_PUBLIC_BASE_URL: &str = "https://generativelanguage.googleapis.com";
pub(super) const DEFAULT_GEMINI_INTERNAL_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub(super) const DEFAULT_GEMINI_INTERNAL_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub(super) const DEFAULT_VERTEX_PUBLIC_BASE_URL: &str = "https://aiplatform.googleapis.com";
pub(super) const GOOGLE_CLOUD_PLATFORM_SCOPE: &str =
    "https://www.googleapis.com/auth/cloud-platform";
pub(super) const GEMINI_CLI_API_CLIENT_HEADER: &str = "google-genai-sdk/1.41.0 gl-node/v22.19.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GeminiPublicPostMethod {
    GenerateContent,
    StreamGenerateContent,
    CountTokens,
}

impl GeminiPublicPostMethod {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::GenerateContent => "generateContent",
            Self::StreamGenerateContent => "streamGenerateContent",
            Self::CountTokens => "countTokens",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GeminiInternalMethod {
    GenerateContent,
    StreamGenerateContent,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GeminiPublicAction {
    pub(super) model: String,
    pub(super) method: GeminiPublicPostMethod,
}

#[derive(Debug, Clone)]
pub(super) struct GeminiPublicRequestBody {
    pub(super) body: Vec<u8>,
    pub(super) user_agent: String,
    pub(super) query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GeminiInternalCredentials {
    pub(super) access_token: String,
    pub(super) project_id: String,
    pub(super) proxy_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VertexServiceAccountCredentials {
    pub(super) access_token: String,
    pub(super) project_id: String,
    pub(super) location: String,
    pub(super) proxy_url: Option<String>,
}

#[derive(Debug)]
pub(super) struct PendingProviderStream {
    pub(super) status: u16,
    pub(super) headers: ReqwestHeaderMap,
    pub(super) response: reqwest::Response,
}

#[derive(Debug, Error)]
pub(super) enum GeminiInternalCallerError {
    #[error(transparent)]
    ReadAuthFile(#[from] AuthFileStoreError),
    #[error("gemini auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("gemini auth is missing access token")]
    MissingAccessToken,
    #[error("gemini auth is missing project_id")]
    MissingProjectId,
    #[error("gemini token refresh request failed: {0}")]
    RefreshRequest(#[from] reqwest::Error),
    #[error("gemini token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("gemini upstream returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug, Error)]
pub(super) enum GeminiPublicCallerError {
    #[error(transparent)]
    ReadAuthFile(#[from] AuthFileStoreError),
    #[error("gemini auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("gemini auth is missing access token")]
    MissingAccessToken,
    #[error("gemini auth is unsupported for public Gemini POST routes")]
    UnsupportedAuthFile,
    #[error("gemini token refresh request failed: {0}")]
    RefreshRequest(#[source] reqwest::Error),
    #[error("gemini token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("gemini upstream request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("gemini upstream returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

fn gemini_cli_user_agent(model: &str) -> String {
    let model = model.trim();
    let model = if model.is_empty() { "unknown" } else { model };
    let os = match std::env::consts::OS {
        "windows" => "win32",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "amd64" => "x64",
        "x86_64" => "x64",
        "x86" => "x86",
        other => other,
    };
    format!("GeminiCLI/0.31.0/{model} ({os}; {arch})")
}
