use crate::{write_auth_file, AuthFileStoreError};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedVertexCredential {
    pub file_name: String,
    pub file_path: PathBuf,
    pub project_id: String,
    pub email: String,
    pub location: String,
}

#[derive(Debug, Error)]
pub enum VertexCredentialImportError {
    #[error("invalid json: {0}")]
    InvalidJson(serde_json::Error),
    #[error("service account json must be an object")]
    InvalidRoot,
    #[error("service account missing private_key")]
    MissingPrivateKey,
    #[error("project_id missing")]
    MissingProjectId,
    #[error("failed to encode auth file: {0}")]
    Encode(serde_json::Error),
    #[error(transparent)]
    Write(#[from] AuthFileStoreError),
}

pub fn import_vertex_credential(
    auth_dir: &Path,
    data: &[u8],
    location: Option<&str>,
) -> Result<ImportedVertexCredential, VertexCredentialImportError> {
    let service_account_value: Value =
        serde_json::from_slice(data).map_err(VertexCredentialImportError::InvalidJson)?;
    let service_account_object = service_account_value
        .as_object()
        .ok_or(VertexCredentialImportError::InvalidRoot)?;

    let normalized_service_account = normalize_service_account(service_account_object)?;
    let project_id = value_as_trimmed_string(normalized_service_account.get("project_id"))
        .ok_or(VertexCredentialImportError::MissingProjectId)?
        .to_string();
    let email = value_as_trimmed_string(normalized_service_account.get("client_email"))
        .unwrap_or_default()
        .to_string();
    let location = location
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("us-central1")
        .to_string();
    let file_name = format!("vertex-{}.json", sanitize_vertex_file_part(&project_id));
    let label = label_for_vertex(&project_id, &email);

    let payload = serde_json::json!({
        "id": file_name,
        "provider": "vertex",
        "type": "vertex",
        "service_account": Value::Object(normalized_service_account),
        "project_id": project_id,
        "email": email,
        "location": location,
        "label": label,
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(VertexCredentialImportError::Encode)?;
    let written_name = write_auth_file(auth_dir, &file_name, &bytes)?;

    Ok(ImportedVertexCredential {
        file_path: auth_dir.join(&written_name),
        file_name: written_name,
        project_id,
        email,
        location,
    })
}

fn normalize_service_account(
    input: &Map<String, Value>,
) -> Result<Map<String, Value>, VertexCredentialImportError> {
    let mut output = input.clone();
    let private_key = value_as_trimmed_string(output.get("private_key"))
        .ok_or(VertexCredentialImportError::MissingPrivateKey)?;
    let normalized_private_key = private_key.replace("\r\n", "\n").replace('\r', "\n");
    output.insert(
        "private_key".to_string(),
        Value::String(normalized_private_key.trim().to_string()),
    );
    Ok(output)
}

fn value_as_trimmed_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn sanitize_vertex_file_part(value: &str) -> String {
    let mut output = value.trim().to_string();
    for (from, to) in [("/", "_"), ("\\", "_"), (":", "_"), (" ", "-")] {
        output = output.replace(from, to);
    }
    if output.is_empty() {
        "vertex".to_string()
    } else {
        output
    }
}

fn label_for_vertex(project_id: &str, email: &str) -> String {
    let project_id = project_id.trim();
    let email = email.trim();
    if !project_id.is_empty() && !email.is_empty() {
        format!("{project_id} ({email})")
    } else if !project_id.is_empty() {
        project_id.to_string()
    } else if !email.is_empty() {
        email.to_string()
    } else {
        "vertex".to_string()
    }
}
