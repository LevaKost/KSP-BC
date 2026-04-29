//! On-the-wire protocol messages exchanged between sender and receiver.
//!
//! Mirrors the `HELLO → READY → META → ACK → DATA → EOF → VERIFY → DONE`
//! sequence described in `docs/protocol.md`.

use serde::{Deserialize, Serialize};

use crate::craft::CraftMetadata;

/// Wire protocol version. Bumped any time the message layout changes.
pub const PROTOCOL_VERSION: u16 = 1;

/// All message variants flowing in either direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolMessage {
    Hello {
        version: u16,
        platform: String,
        client: String,
    },
    Ready {
        accept: bool,
        reason: Option<String>,
    },
    Meta(CraftMetadata),
    Ack,
    Data {
        offset: u64,
        bytes: Vec<u8>,
    },
    Eof,
    Verify {
        sha256_ok: bool,
    },
    Done,
    Error {
        message: String,
    },
}

impl ProtocolMessage {
    pub fn kind(&self) -> &'static str {
        match self {
            ProtocolMessage::Hello { .. } => "HELLO",
            ProtocolMessage::Ready { .. } => "READY",
            ProtocolMessage::Meta(_) => "META",
            ProtocolMessage::Ack => "ACK",
            ProtocolMessage::Data { .. } => "DATA",
            ProtocolMessage::Eof => "EOF",
            ProtocolMessage::Verify { .. } => "VERIFY",
            ProtocolMessage::Done => "DONE",
            ProtocolMessage::Error { .. } => "ERROR",
        }
    }
}

/// Build the `HELLO` we send on every connection.
pub fn local_hello() -> ProtocolMessage {
    ProtocolMessage::Hello {
        version: PROTOCOL_VERSION,
        platform: std::env::consts::OS.to_string(),
        client: format!("ksp-share/{}", env!("CARGO_PKG_VERSION")),
    }
}
