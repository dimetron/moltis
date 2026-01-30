/// Config schema types (agents, channels, tools, session, gateway, plugins).
/// Corresponds to `src/config/types.ts` and `zod-schema.*.ts` in the TS codebase.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Root configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MoltisConfig {
    pub providers: ProvidersConfig,
    pub tools: ToolsConfig,
}

/// Tools configuration (exec, sandbox, policy).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub exec: ExecConfig,
    pub policy: ToolPolicyConfig,
}

/// Exec tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecConfig {
    pub default_timeout_secs: u64,
    pub max_output_bytes: usize,
    pub approval_mode: String,
    pub security_level: String,
    pub allowlist: Vec<String>,
    pub sandbox: SandboxConfig,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 30,
            max_output_bytes: 200 * 1024,
            approval_mode: "on-miss".into(),
            security_level: "allowlist".into(),
            allowlist: Vec::new(),
            sandbox: SandboxConfig::default(),
        }
    }
}

/// Sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxConfig {
    pub mode: String,
    pub scope: String,
    pub workspace_mount: String,
    pub image: Option<String>,
    pub container_prefix: Option<String>,
    pub no_network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            scope: "session".into(),
            workspace_mount: "ro".into(),
            image: None,
            container_prefix: None,
            no_network: true,
        }
    }
}

/// Tool policy configuration (allow/deny lists).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolPolicyConfig {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub profile: Option<String>,
}

/// OAuth provider configuration (e.g. openai-codex).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub callback_port: u16,
}

/// LLM provider configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    /// Provider-specific settings keyed by provider name.
    /// Known keys: "anthropic", "openai", "gemini", "groq", "xai", "deepseek"
    #[serde(flatten)]
    pub providers: HashMap<String, ProviderEntry>,
}

/// Configuration for a single LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderEntry {
    /// Whether this provider is enabled. Defaults to true.
    pub enabled: bool,

    /// Override the API key (optional; env var still takes precedence if set).
    pub api_key: Option<String>,

    /// Override the base URL.
    pub base_url: Option<String>,

    /// Default model ID for this provider.
    pub model: Option<String>,
}

impl Default for ProviderEntry {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key: None,
            base_url: None,
            model: None,
        }
    }
}

impl ProvidersConfig {
    /// Check if a provider is enabled (defaults to true if not configured).
    pub fn is_enabled(&self, name: &str) -> bool {
        self.providers.get(name).is_none_or(|e| e.enabled)
    }

    /// Get the configured entry for a provider, if any.
    pub fn get(&self, name: &str) -> Option<&ProviderEntry> {
        self.providers.get(name)
    }
}
