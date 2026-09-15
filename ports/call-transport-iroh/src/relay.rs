//! The relay this endpoint camps on — the Rust of `CroftRelay.kt`.
//!
//! Both of our ports are non-standard (the relay's front page advertises
//! "connect via https://relay.croft.ing:8443 (relay) and udp/7824 (QUIC)"),
//! which is why this is a [`RelayTarget`] carrying a QUIC port and not a bare
//! URL: a URL alone would probe address discovery on iroh's default port and
//! miss.
//!
//! The admission token rides in the relay config as `auth_token` and becomes
//! an `Authorization: Bearer` header on the relay upgrade request. iroh reads
//! it when it STARTS the relay connection (`start_active_relay`, read in
//! iroh 1.1's relay actor); a live connection keeps the token it was opened
//! with, and `Endpoint::insert_relay` only re-runs address discovery. So
//! changing the token is a stop/start of the endpoint — the same lifecycle
//! fact `CallPeer.rebindWithToken` was built around, and the reason R0's
//! rule matters at all.

use std::sync::Arc;

use iroh::{RelayConfig, RelayMap, RelayUrl};
use iroh_relay::RelayQuicConfig;

use crate::CallTransportError;

/// The production relay (enforce since the 2026-08-30 flip).
pub const CROFT_RELAY_URL: &str = "https://relay.croft.ing:8443";
/// The staging enforce listener — the rehearsal seat for the next relay
/// artifact, verifying the STAGING mint key.
pub const CROFT_STAGING_RELAY_URL: &str = "https://relay.croft.ing:8444";
/// The relay's QUIC address-discovery port, shared by both listeners.
pub const CROFT_RELAY_QUIC_PORT: u16 = 7824;

/// A relay to camp on: its URL and, if it does QUIC address discovery, the
/// port that runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayTarget {
    url: String,
    quic_port: Option<u16>,
}

impl RelayTarget {
    /// A relay at `url`, with no QUIC address discovery.
    pub fn new(url: impl Into<String>) -> Self {
        RelayTarget {
            url: url.into(),
            quic_port: None,
        }
    }

    /// The same relay, also used for QUIC address discovery on `port`.
    #[must_use]
    pub fn with_quic_port(mut self, port: u16) -> Self {
        self.quic_port = Some(port);
        self
    }

    /// Our production relay, both ports.
    #[must_use]
    pub fn croft_production() -> Self {
        RelayTarget::new(CROFT_RELAY_URL).with_quic_port(CROFT_RELAY_QUIC_PORT)
    }

    /// Our staging enforce listener, both ports.
    #[must_use]
    pub fn croft_staging() -> Self {
        RelayTarget::new(CROFT_STAGING_RELAY_URL).with_quic_port(CROFT_RELAY_QUIC_PORT)
    }

    /// The relay's URL as supplied.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn parsed_url(&self) -> Result<RelayUrl, CallTransportError> {
        self.url.parse().map_err(
            |e: iroh::RelayUrlParseError| CallTransportError::BadRelayUrl {
                url: self.url.clone(),
                reason: e.to_string(),
            },
        )
    }

    /// The one-entry relay map an endpoint binds with, `token` riding along
    /// as the relay's `auth_token`.
    pub(crate) fn relay_map(&self, token: Option<&str>) -> Result<RelayMap, CallTransportError> {
        let url = self.parsed_url()?;
        let mut config = RelayConfig::new(url.clone(), self.quic_port.map(RelayQuicConfig::new));
        if let Some(token) = token {
            config = config.with_auth_token(token);
        }
        let map = RelayMap::empty();
        map.insert(url, Arc::new(config));
        Ok(map)
    }
}
