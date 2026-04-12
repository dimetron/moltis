//! Per-account runtime state for Nostr.

use std::sync::Arc;

use {
    moltis_channels::otp::OtpState,
    nostr_sdk::prelude::{Client, Keys, PublicKey, ToBech32},
    tokio::sync::RwLock,
    tokio_util::sync::CancellationToken,
};

use crate::config::NostrAccountConfig;

/// Shared config reference — the bus loop and plugin both read/write through
/// this same `Arc` so runtime config updates (DM policy, allowlist) take
/// effect immediately without restarting the account.
pub type SharedConfig = Arc<RwLock<NostrAccountConfig>>;

/// Runtime state for a single active Nostr account.
pub struct AccountState {
    /// The nostr-sdk client connected to relays.
    pub client: Client,
    /// Bot key pair (secret + public).
    pub keys: Keys,
    /// Shared account configuration — same Arc given to the bus loop.
    pub config: SharedConfig,
    /// Pre-parsed allowlist pubkeys, refreshed on config update.
    pub cached_allowlist: Arc<RwLock<Vec<PublicKey>>>,
    /// Cancellation token for the subscription loop.
    pub cancel: CancellationToken,
    /// OTP self-approval state for non-allowlisted senders.
    pub otp: OtpState,
}

impl std::fmt::Debug for AccountState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pk = self
            .keys
            .public_key()
            .to_bech32()
            .unwrap_or_else(|_| self.keys.public_key().to_hex());
        f.debug_struct("AccountState")
            .field("pubkey", &pk)
            .finish_non_exhaustive()
    }
}
