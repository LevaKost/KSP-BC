//! Transport layer.
//!
//! Stage 1 ships the [`tcp`] transport. Stage 3 adds [`mdns`] for LAN
//! auto-discovery on top of the same TCP wire format. QUIC P2P will
//! land alongside both in a follow-up.

pub mod mdns;
#[cfg(feature = "p2p")]
pub mod p2p;
pub mod tcp;
