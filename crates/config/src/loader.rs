use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use tracing::{debug, warn};

use crate::{env_subst::substitute_env, schema::MoltisConfig};

/// Standard config file names, checked in order.
const CONFIG_FILENAMES: &[&str] = &["moltis.toml", "moltis.yaml", "moltis.yml", "moltis.json"];

/// Override for the config directory, set via `set_config_dir()`.
static CONFIG_DIR_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Set a custom config directory. When set, config discovery only looks in
/// this directory (project-local and user-global paths are skipped).
/// Can be called multiple times (e.g. in tests) — each call replaces the
/// previous override.
pub fn set_config_dir(path: PathBuf) {
    *CONFIG_DIR_OVERRIDE.lock().unwrap() = Some(path);
}

/// Clear the config directory override, restoring default discovery.
pub fn clear_config_dir() {
    *CONFIG_DIR_OVERRIDE.lock().unwrap() = None;
}

fn config_dir_override() -> Option<PathBuf> {
    CONFIG_DIR_OVERRIDE.lock().unwrap().clone()
}

/// Load config from the given path (any supported format).
pub fn load_config(path: &Path) -> anyhow::Result<MoltisConfig> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let raw = substitute_env(&raw);
    parse_config(&raw, path)
}

/// Load and parse the config file with env substitution and includes.
pub fn load_config_value(path: &Path) -> anyhow::Result<serde_json::Value> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let raw = substitute_env(&raw);
    parse_config_value(&raw, path)
}

/// Discover and load config from standard locations.
///
/// Search order:
/// 1. `./moltis.{toml,yaml,yml,json}` (project-local)
/// 2. `~/.config/moltis/moltis.{toml,yaml,yml,json}` (user-global)
///
/// Returns `MoltisConfig::default()` if no config file is found.
pub fn discover_and_load() -> MoltisConfig {
    if let Some(path) = find_config_file() {
        debug!(path = %path.display(), "loading config");
        match load_config(&path) {
            Ok(cfg) => return cfg,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to load config, using defaults");
            },
        }
    } else {
        debug!("no config file found, writing default config");
        let config = MoltisConfig::default();
        if let Err(e) = write_default_config(&config) {
            warn!(error = %e, "failed to write default config file");
        }
        return config;
    }
    MoltisConfig::default()
}

/// Find the first config file in standard locations.
///
/// When a config dir override is set, only that directory is searched —
/// project-local and user-global paths are skipped for isolation.
fn find_config_file() -> Option<PathBuf> {
    if let Some(dir) = config_dir_override() {
        for name in CONFIG_FILENAMES {
            let p = dir.join(name);
            if p.exists() {
                return Some(p);
            }
        }
        // Override is set — don't fall through to other locations.
        return None;
    }

    // Project-local
    for name in CONFIG_FILENAMES {
        let p = PathBuf::from(name);
        if p.exists() {
            return Some(p);
        }
    }

    // User-global: ~/.config/moltis/
    if let Some(dir) = home_dir().map(|h| h.join(".config").join("moltis")) {
        for name in CONFIG_FILENAMES {
            let p = dir.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Returns the config directory: override, or `~/.config/moltis/` on all platforms.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = config_dir_override() {
        return Some(dir);
    }
    home_dir().map(|h| h.join(".config").join("moltis"))
}

/// Returns the data directory: `~/.moltis/` on all platforms.
pub fn data_dir() -> PathBuf {
    home_dir()
        .map(|h| h.join(".moltis"))
        .unwrap_or_else(|| PathBuf::from(".moltis"))
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

/// Returns the path of an existing config file, or the default TOML path.
pub fn find_or_default_config_path() -> PathBuf {
    if let Some(path) = find_config_file() {
        return path;
    }
    config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("moltis.toml")
}

/// Lock guarding config read-modify-write cycles.
static CONFIG_SAVE_LOCK: Mutex<()> = Mutex::new(());

/// Atomically load the current config, apply `f`, and save.
///
/// Acquires a process-wide lock so concurrent callers cannot race.
/// Returns the path written to.
pub fn update_config(f: impl FnOnce(&mut MoltisConfig)) -> anyhow::Result<PathBuf> {
    let _guard = CONFIG_SAVE_LOCK.lock().unwrap();
    let mut config = discover_and_load();
    f(&mut config);
    save_config_inner(&config)
}

/// Serialize `config` to TOML and write it to the user-global config path.
///
/// Creates parent directories if needed. Returns the path written to.
///
/// Prefer [`update_config`] for read-modify-write cycles to avoid races.
pub fn save_config(config: &MoltisConfig) -> anyhow::Result<PathBuf> {
    let _guard = CONFIG_SAVE_LOCK.lock().unwrap();
    save_config_inner(config)
}

fn save_config_inner(config: &MoltisConfig) -> anyhow::Result<PathBuf> {
    let path = find_or_default_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str =
        toml::to_string_pretty(config).map_err(|e| anyhow::anyhow!("serialize config: {e}"))?;
    std::fs::write(&path, toml_str)?;
    debug!(path = %path.display(), "saved config");
    Ok(path)
}

/// Write the default config file to the user-global config path.
/// Only called when no config file exists yet.
fn write_default_config(config: &MoltisConfig) -> anyhow::Result<()> {
    let path = find_or_default_config_path();
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str =
        toml::to_string_pretty(config).map_err(|e| anyhow::anyhow!("serialize config: {e}"))?;
    std::fs::write(&path, &toml_str)?;
    debug!(path = %path.display(), "wrote default config file");
    Ok(())
}

fn parse_config(raw: &str, path: &Path) -> anyhow::Result<MoltisConfig> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("toml");

    match ext {
        "toml" => Ok(toml::from_str(raw)?),
        "yaml" | "yml" => Ok(serde_yaml::from_str(raw)?),
        "json" => Ok(serde_json::from_str(raw)?),
        _ => anyhow::bail!("unsupported config format: .{ext}"),
    }
}

fn parse_config_value(raw: &str, path: &Path) -> anyhow::Result<serde_json::Value> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("toml");

    match ext {
        "toml" => {
            let v: toml::Value = toml::from_str(raw)?;
            Ok(serde_json::to_value(v)?)
        },
        "yaml" | "yml" => {
            let v: serde_yaml::Value = serde_yaml::from_str(raw)?;
            Ok(serde_json::to_value(v)?)
        },
        "json" => Ok(serde_json::from_str(raw)?),
        _ => anyhow::bail!("unsupported config format: .{ext}"),
    }
}
