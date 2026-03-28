use std::sync::atomic::Ordering;

use {
    axum::{
        Json,
        extract::{Path, State},
        http::StatusCode,
        response::{IntoResponse, Response},
    },
    serde::Serialize,
    tokio::process::Command,
};

use moltis_gateway::{
    auth::{SshAuthMode, SshKeyEntry, SshTargetEntry},
    node_exec::exec_resolved_ssh_target,
};

const SSH_STORE_UNAVAILABLE: &str = "SSH_STORE_UNAVAILABLE";
const SSH_KEY_NAME_REQUIRED: &str = "SSH_KEY_NAME_REQUIRED";
const SSH_PRIVATE_KEY_REQUIRED: &str = "SSH_PRIVATE_KEY_REQUIRED";
const SSH_TARGET_LABEL_REQUIRED: &str = "SSH_TARGET_LABEL_REQUIRED";
const SSH_TARGET_REQUIRED: &str = "SSH_TARGET_REQUIRED";
const SSH_LIST_FAILED: &str = "SSH_LIST_FAILED";
const SSH_KEY_GENERATE_FAILED: &str = "SSH_KEY_GENERATE_FAILED";
const SSH_KEY_IMPORT_FAILED: &str = "SSH_KEY_IMPORT_FAILED";
const SSH_KEY_DELETE_FAILED: &str = "SSH_KEY_DELETE_FAILED";
const SSH_TARGET_CREATE_FAILED: &str = "SSH_TARGET_CREATE_FAILED";
const SSH_TARGET_DELETE_FAILED: &str = "SSH_TARGET_DELETE_FAILED";
const SSH_TARGET_DEFAULT_FAILED: &str = "SSH_TARGET_DEFAULT_FAILED";
const SSH_TARGET_TEST_FAILED: &str = "SSH_TARGET_TEST_FAILED";

#[derive(Serialize)]
pub struct SshStatusResponse {
    keys: Vec<SshKeyEntry>,
    targets: Vec<SshTargetEntry>,
}

impl IntoResponse for SshStatusResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize)]
pub struct SshMutationResponse {
    ok: bool,
    id: Option<i64>,
}

impl SshMutationResponse {
    fn success(id: Option<i64>) -> Self {
        Self { ok: true, id }
    }
}

impl IntoResponse for SshMutationResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize)]
pub struct SshTestResponse {
    ok: bool,
    reachable: bool,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

impl IntoResponse for SshTestResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn service_unavailable(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code,
            message: message.into(),
        }
    }

    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn internal(code: &'static str, err: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body {
            code: &'static str,
            error: String,
        }

        (
            self.status,
            Json(Body {
                code: self.code,
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(serde::Deserialize)]
pub struct GenerateKeyRequest {
    name: String,
}

#[derive(serde::Deserialize)]
pub struct ImportKeyRequest {
    name: String,
    private_key: String,
}

#[derive(serde::Deserialize)]
pub struct CreateTargetRequest {
    label: String,
    target: String,
    port: Option<u16>,
    auth_mode: SshAuthMode,
    key_id: Option<i64>,
    #[serde(default)]
    is_default: bool,
}

pub async fn ssh_status(
    State(state): State<crate::server::AppState>,
) -> Result<SshStatusResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    let keys = store
        .list_ssh_keys()
        .await
        .map_err(|err| ApiError::internal(SSH_LIST_FAILED, err))?;
    let targets = store
        .list_ssh_targets()
        .await
        .map_err(|err| ApiError::internal(SSH_LIST_FAILED, err))?;
    Ok(SshStatusResponse { keys, targets })
}

pub async fn ssh_generate_key(
    State(state): State<crate::server::AppState>,
    Json(body): Json<GenerateKeyRequest>,
) -> Result<SshMutationResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            SSH_KEY_NAME_REQUIRED,
            "ssh key name is required",
        ));
    }

    let (private_key, public_key, fingerprint) = generate_ssh_key_material(name)
        .await
        .map_err(|err| ApiError::internal(SSH_KEY_GENERATE_FAILED, err))?;
    let id = store
        .create_ssh_key(name, &private_key, &public_key, &fingerprint)
        .await
        .map_err(|err| ApiError::internal(SSH_KEY_GENERATE_FAILED, err))?;

    Ok(SshMutationResponse::success(Some(id)))
}

pub async fn ssh_import_key(
    State(state): State<crate::server::AppState>,
    Json(body): Json<ImportKeyRequest>,
) -> Result<SshMutationResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            SSH_KEY_NAME_REQUIRED,
            "ssh key name is required",
        ));
    }
    if body.private_key.trim().is_empty() {
        return Err(ApiError::bad_request(
            SSH_PRIVATE_KEY_REQUIRED,
            "private key is required",
        ));
    }

    let (public_key, fingerprint) = inspect_imported_private_key(&body.private_key)
        .await
        .map_err(|err| ApiError::bad_request(SSH_KEY_IMPORT_FAILED, err.to_string()))?;
    let id = store
        .create_ssh_key(name, &body.private_key, &public_key, &fingerprint)
        .await
        .map_err(|err| ApiError::internal(SSH_KEY_IMPORT_FAILED, err))?;

    Ok(SshMutationResponse::success(Some(id)))
}

pub async fn ssh_delete_key(
    State(state): State<crate::server::AppState>,
    Path(id): Path<i64>,
) -> Result<SshMutationResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    store
        .delete_ssh_key(id)
        .await
        .map_err(|err| ApiError::bad_request(SSH_KEY_DELETE_FAILED, err.to_string()))?;
    Ok(SshMutationResponse::success(None))
}

pub async fn ssh_create_target(
    State(state): State<crate::server::AppState>,
    Json(body): Json<CreateTargetRequest>,
) -> Result<SshMutationResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    if body.label.trim().is_empty() {
        return Err(ApiError::bad_request(
            SSH_TARGET_LABEL_REQUIRED,
            "target label is required",
        ));
    }
    if body.target.trim().is_empty() {
        return Err(ApiError::bad_request(
            SSH_TARGET_REQUIRED,
            "target is required",
        ));
    }

    let id = store
        .create_ssh_target(
            &body.label,
            &body.target,
            body.port,
            body.auth_mode,
            body.key_id,
            body.is_default,
        )
        .await
        .map_err(|err| ApiError::bad_request(SSH_TARGET_CREATE_FAILED, err.to_string()))?;
    refresh_ssh_target_count(&state).await;

    Ok(SshMutationResponse::success(Some(id)))
}

pub async fn ssh_delete_target(
    State(state): State<crate::server::AppState>,
    Path(id): Path<i64>,
) -> Result<SshMutationResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    store
        .delete_ssh_target(id)
        .await
        .map_err(|err| ApiError::internal(SSH_TARGET_DELETE_FAILED, err))?;
    refresh_ssh_target_count(&state).await;

    Ok(SshMutationResponse::success(None))
}

pub async fn ssh_set_default_target(
    State(state): State<crate::server::AppState>,
    Path(id): Path<i64>,
) -> Result<SshMutationResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    store
        .set_default_ssh_target(id)
        .await
        .map_err(|err| ApiError::bad_request(SSH_TARGET_DEFAULT_FAILED, err.to_string()))?;
    Ok(SshMutationResponse::success(Some(id)))
}

pub async fn ssh_test_target(
    State(state): State<crate::server::AppState>,
    Path(id): Path<i64>,
) -> Result<SshTestResponse, ApiError> {
    let store = state.gateway.credential_store.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(SSH_STORE_UNAVAILABLE, "no credential store")
    })?;

    let target = store
        .resolve_ssh_target_by_id(id)
        .await
        .map_err(|err| ApiError::internal(SSH_TARGET_TEST_FAILED, err))?
        .ok_or_else(|| ApiError::bad_request(SSH_TARGET_TEST_FAILED, "ssh target not found"))?;

    let probe = "__moltis_ssh_probe__";
    let result = exec_resolved_ssh_target(
        store,
        &target,
        &format!("printf {probe}"),
        10,
        None,
        None,
        8 * 1024,
    )
    .await
    .map_err(|err| ApiError::bad_request(SSH_TARGET_TEST_FAILED, err.to_string()))?;

    Ok(SshTestResponse {
        ok: true,
        reachable: result.exit_code == 0 && result.stdout.contains(probe),
        stdout: result.stdout,
        stderr: result.stderr,
        exit_code: result.exit_code,
    })
}

async fn refresh_ssh_target_count(state: &crate::server::AppState) {
    let Some(store) = state.gateway.credential_store.as_ref() else {
        return;
    };
    match store.ssh_target_count().await {
        Ok(count) => state
            .gateway
            .ssh_target_count
            .store(count, Ordering::Relaxed),
        Err(error) => tracing::warn!(%error, "failed to refresh ssh target count"),
    }
}

async fn generate_ssh_key_material(name: &str) -> anyhow::Result<(String, String, String)> {
    let dir = tempfile::tempdir()?;
    let key_path = dir.path().join("moltis_deploy_key");
    let output = Command::new("ssh-keygen")
        .arg("-t")
        .arg("ed25519")
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg(format!("moltis:{name}"))
        .arg("-f")
        .arg(&key_path)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        );
    }

    let private_key: String = tokio::fs::read_to_string(&key_path).await?;
    let public_key: String = tokio::fs::read_to_string(key_path.with_extension("pub")).await?;
    let fingerprint = ssh_keygen_fingerprint(&key_path).await?;
    Ok((private_key, public_key.trim().to_string(), fingerprint))
}

async fn inspect_imported_private_key(private_key: &str) -> anyhow::Result<(String, String)> {
    let dir = tempfile::tempdir()?;
    let key_path = dir.path().join("imported_key");
    tokio::fs::write(&key_path, private_key).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
    }

    let public_output = Command::new("ssh-keygen")
        .arg("-y")
        .arg("-f")
        .arg(&key_path)
        .output()
        .await?;
    if !public_output.status.success() {
        let stderr = String::from_utf8_lossy(&public_output.stderr)
            .trim()
            .to_string();
        anyhow::bail!(if stderr.to_lowercase().contains("passphrase") {
            "passphrase-protected private keys are not supported yet".to_string()
        } else {
            stderr
        });
    }

    let fingerprint = ssh_keygen_fingerprint(&key_path).await?;
    let public_key = String::from_utf8(public_output.stdout)?.trim().to_string();
    Ok((public_key, fingerprint))
}

async fn ssh_keygen_fingerprint(path: &std::path::Path) -> anyhow::Result<String> {
    let output = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(path)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test]
    async fn generated_key_material_round_trips() {
        let (private_key, public_key, fingerprint) =
            generate_ssh_key_material("test-key").await.unwrap();
        assert!(private_key.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(public_key.starts_with("ssh-ed25519 "));
        assert!(fingerprint.contains("SHA256:"));
    }

    #[tokio::test]
    async fn imported_key_is_validated() {
        let (private_key, ..) = generate_ssh_key_material("importable").await.unwrap();
        let (public_key, fingerprint) = inspect_imported_private_key(&private_key).await.unwrap();
        assert!(public_key.starts_with("ssh-ed25519 "));
        assert!(fingerprint.contains("SHA256:"));
    }
}
