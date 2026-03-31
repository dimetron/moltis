//! HTTP routes for ngrok tunnel configuration and runtime status.

use {
    axum::{
        Json, Router,
        extract::State,
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post},
    },
    secrecy::Secret,
    serde::Deserialize,
};

use crate::server::AppState;

const NGROK_CONFIG_INVALID: &str = "NGROK_CONFIG_INVALID";
const NGROK_SAVE_FAILED: &str = "NGROK_SAVE_FAILED";
const NGROK_APPLY_FAILED: &str = "NGROK_APPLY_FAILED";

fn ngrok_error(code: &str, error: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "error": error.into(),
    })
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn authtoken_source(config: &moltis_config::NgrokConfig) -> Option<&'static str> {
    if config.authtoken.is_some() {
        Some("config")
    } else if std::env::var_os("NGROK_AUTHTOKEN").is_some() {
        Some("env")
    } else {
        None
    }
}

async fn status_payload(state: &AppState) -> serde_json::Value {
    let config = moltis_config::discover_and_load();
    let runtime = state.ngrok_runtime.read().await.clone();
    let authtoken_source = authtoken_source(&config.ngrok);
    let runtime_active = runtime.is_some();

    serde_json::json!({
        "enabled": config.ngrok.enabled,
        "domain": config.ngrok.domain,
        "authtoken_present": authtoken_source.is_some(),
        "authtoken_source": authtoken_source,
        "public_url": runtime.as_ref().map(|status| status.public_url.clone()),
        "passkey_warning": runtime.and_then(|status| status.passkey_warning),
        "runtime_active": runtime_active,
    })
}

#[derive(Deserialize)]
struct SaveNgrokConfigRequest {
    enabled: bool,
    #[serde(default)]
    authtoken: Option<String>,
    #[serde(default)]
    clear_authtoken: bool,
    #[serde(default)]
    domain: Option<String>,
}

/// Build the ngrok API router.
pub fn ngrok_router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status_handler))
        .route("/config", post(save_config_handler))
}

async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(status_payload(&state).await).into_response()
}

async fn save_config_handler(
    State(state): State<AppState>,
    Json(body): Json<SaveNgrokConfigRequest>,
) -> impl IntoResponse {
    let existing = moltis_config::discover_and_load();
    let domain = normalize_optional(body.domain.as_deref());
    let new_authtoken = normalize_optional(body.authtoken.as_deref());

    let token_will_exist = if body.clear_authtoken {
        new_authtoken.is_some() || std::env::var_os("NGROK_AUTHTOKEN").is_some()
    } else {
        new_authtoken.is_some()
            || existing.ngrok.authtoken.is_some()
            || std::env::var_os("NGROK_AUTHTOKEN").is_some()
    };

    if body.enabled && !token_will_exist {
        return (
            StatusCode::BAD_REQUEST,
            Json(ngrok_error(
                NGROK_CONFIG_INVALID,
                "ngrok requires an authtoken in config or NGROK_AUTHTOKEN in the environment",
            )),
        )
            .into_response();
    }

    if let Err(error) = moltis_config::update_config(|config| {
        config.ngrok.enabled = body.enabled;
        config.ngrok.domain = domain.clone();

        if body.clear_authtoken {
            config.ngrok.authtoken = None;
        }
        if let Some(authtoken) = new_authtoken.as_ref() {
            config.ngrok.authtoken = Some(Secret::new(authtoken.clone()));
        }
    }) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ngrok_error(
                NGROK_SAVE_FAILED,
                format!("failed to save ngrok config: {error}"),
            )),
        )
            .into_response();
    }

    let updated = moltis_config::discover_and_load();
    if let Some(controller) = state.ngrok_controller.upgrade()
        && let Err(error) = controller.apply(&updated.ngrok).await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": NGROK_APPLY_FAILED,
                "error": format!("saved ngrok config but failed to apply it: {error}"),
                "status": status_payload(&state).await,
            })),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "ok": true,
        "status": status_payload(&state).await,
    }))
    .into_response()
}
