//! Channel plugin system.
//!
//! Each channel (Telegram, Discord, Slack, WhatsApp, etc.) implements the
//! ChannelPlugin trait with sub-traits for config, auth, inbound/outbound
//! messaging, status, and gateway lifecycle.

pub mod gating;
pub mod plugin;
pub mod registry;

pub use plugin::{ChannelOutbound, ChannelPlugin, ChannelStatus};
