//! Configuration loading, validation, env substitution, and legacy migration.
//!
//! Config files: `moltis.toml`, `moltis.yaml`, or `moltis.json`
//! Searched in `./` then `~/.config/moltis/`.
//!
//! Supports `${ENV_VAR}` substitution in all string values.

pub mod env_subst;
pub mod loader;
pub mod migrate;
pub mod schema;

pub use {
    loader::{config_dir, discover_and_load, find_or_default_config_path, save_config},
    schema::{AgentIdentity, MoltisConfig, UserProfile},
};
