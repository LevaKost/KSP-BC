//! High-level send/receive orchestration.

mod handshake;
#[cfg(feature = "p2p")]
pub mod quic;
mod receiver;
mod sender;

pub use handshake::{ProtocolMessage, PROTOCOL_VERSION};
pub use receiver::{receive_blueprint, ReceiveOptions};
pub use sender::{send_blueprint, SendOptions};

/// Default chunk size for `DATA` frames (64 KiB, per the protocol spec).
pub const DEFAULT_CHUNK_BYTES: usize = 64 * 1024;
