//! QUIC-based P2P transport via [`iroh`].
//!
//! When the `p2p` Cargo feature is enabled, `ksp-share` can dial peers
//! by their iroh `EndpointAddr` (a NodeID + optional direct addresses
//! and relay URL) instead of an IP/port pair. This gives us NAT hole
//! punching for free and falls back to a public relay server when
//! direct UDP isn't possible.
//!
//! The protocol over the wire is the same `HELLO → META → READY →
//! DATA → EOF → VERIFY → DONE` flow defined in `docs/protocol.md`,
//! framed identically (`u32_be length + bincode(payload)`); the only
//! difference is that the transport is a single QUIC bidirectional
//! stream instead of a TCP socket.

use std::str::FromStr;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl, TransportAddr};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{Error, Result};

/// Application-Layer Protocol Negotiation (ALPN) string identifying
/// the BlueprintShare wire protocol over QUIC.
pub const ALPN: &[u8] = b"ksp-share/v1";

/// Hard ceiling on a single framed message (8 MiB).
const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

/// A serialised peer ticket suitable for sharing out-of-band (paste it
/// into chat, scan it from a QR code, etc.). The format is intentionally
/// stable so future versions can keep parsing tickets emitted by older
/// builds.
///
/// Wire format (`base32` is just an opaque blob, never inspected by the
/// parser):
///
/// ```text
/// ksp-share://<endpoint_id>?relay=<url>&direct=<host:port>&direct=<host:port>...
/// ```
#[derive(Debug, Clone)]
pub struct PeerTicket {
    pub endpoint_id: EndpointId,
    pub relay: Option<RelayUrl>,
    pub direct: Vec<std::net::SocketAddr>,
}

impl PeerTicket {
    /// Build a ticket from an [`EndpointAddr`] reported by a freshly
    /// bound iroh [`Endpoint`].
    pub fn from_addr(addr: &EndpointAddr) -> Self {
        let mut relay = None;
        let mut direct = Vec::new();
        for transport in &addr.addrs {
            match transport {
                TransportAddr::Relay(url) => relay = Some(url.clone()),
                TransportAddr::Ip(addr) => direct.push(*addr),
                _ => {}
            }
        }
        PeerTicket {
            endpoint_id: addr.id,
            relay,
            direct,
        }
    }

    /// Convert the ticket back into an [`EndpointAddr`] suitable for
    /// `Endpoint::connect`.
    pub fn to_addr(&self) -> EndpointAddr {
        let mut addrs: Vec<TransportAddr> = Vec::new();
        if let Some(relay) = &self.relay {
            addrs.push(TransportAddr::Relay(relay.clone()));
        }
        for direct in &self.direct {
            addrs.push(TransportAddr::Ip(*direct));
        }
        EndpointAddr {
            id: self.endpoint_id,
            addrs: addrs.into_iter().collect(),
        }
    }

    /// Human-readable single-line ticket. The format is
    /// `ksp-share://<id>?relay=<url>&direct=<addr>...`.
    pub fn encode(&self) -> String {
        let mut out = format!("ksp-share://{}", self.endpoint_id);
        let mut sep = '?';
        if let Some(relay) = &self.relay {
            out.push(sep);
            out.push_str("relay=");
            out.push_str(relay.as_str());
            sep = '&';
        }
        for direct in &self.direct {
            out.push(sep);
            out.push_str("direct=");
            out.push_str(&direct.to_string());
            sep = '&';
        }
        out
    }

    /// Parse a ticket emitted by [`encode`].
    pub fn decode(input: &str) -> Result<Self> {
        let rest = input
            .strip_prefix("ksp-share://")
            .ok_or_else(|| Error::Protocol("ticket missing `ksp-share://` prefix".into()))?;
        let (id_part, query) = match rest.split_once('?') {
            Some((id, q)) => (id, Some(q)),
            None => (rest, None),
        };
        let endpoint_id = EndpointId::from_str(id_part)
            .map_err(|err| Error::Protocol(format!("invalid endpoint id: {err}")))?;
        let mut relay: Option<RelayUrl> = None;
        let mut direct: Vec<std::net::SocketAddr> = Vec::new();
        if let Some(query) = query {
            for kv in query.split('&').filter(|s| !s.is_empty()) {
                let (k, v) = kv
                    .split_once('=')
                    .ok_or_else(|| Error::Protocol(format!("malformed ticket field: {kv}")))?;
                match k {
                    "relay" => {
                        relay =
                            Some(RelayUrl::from_str(v).map_err(|err| {
                                Error::Protocol(format!("invalid relay url: {err}"))
                            })?);
                    }
                    "direct" => {
                        direct.push(v.parse().map_err(|err| {
                            Error::Protocol(format!("invalid direct addr `{v}`: {err}"))
                        })?);
                    }
                    other => {
                        return Err(Error::Protocol(format!("unknown ticket field `{other}`")));
                    }
                }
            }
        }
        Ok(PeerTicket {
            endpoint_id,
            relay,
            direct,
        })
    }
}

/// Build a fresh iroh endpoint speaking our ALPN. The endpoint is
/// returned as soon as it is bound — discovery against the relay
/// network may still be ongoing. Call [`Endpoint::online`] yourself if
/// you need the endpoint's published [`EndpointAddr`] to be complete
/// (e.g. before printing a ticket).
pub async fn bind_endpoint() -> Result<Endpoint> {
    let endpoint = Endpoint::bind(iroh::endpoint::presets::N0)
        .await
        .map_err(map_err)?;
    endpoint.set_alpns(vec![ALPN.to_vec()]);
    Ok(endpoint)
}

/// Encode and send a value as a length-prefixed bincode frame on a
/// QUIC unidirectional or bidirectional send stream.
pub async fn send_frame<S>(send: &mut SendStream, value: &S) -> Result<()>
where
    S: Serialize,
{
    let payload = bincode::serialize(value)?;
    if payload.len() as u64 > MAX_FRAME_BYTES as u64 {
        return Err(Error::TooLarge {
            size: payload.len() as u64,
            limit: MAX_FRAME_BYTES as u64,
        });
    }
    let len = (payload.len() as u32).to_be_bytes();
    send.write_all(&len).await.map_err(map_io_err)?;
    send.write_all(&payload).await.map_err(map_io_err)?;
    Ok(())
}

/// Receive and decode a length-prefixed bincode frame from a QUIC recv
/// stream.
pub async fn recv_frame<D>(recv: &mut RecvStream) -> Result<D>
where
    D: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await.map_err(map_io_err)?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        return Err(Error::TooLarge {
            size: len as u64,
            limit: MAX_FRAME_BYTES as u64,
        });
    }
    let mut buf = vec![0u8; len as usize];
    recv.read_exact(&mut buf).await.map_err(map_io_err)?;
    let value = bincode::deserialize(&buf)?;
    Ok(value)
}

/// Accept the next bidirectional stream on a QUIC connection.
pub async fn accept_bi(conn: &Connection) -> Result<(SendStream, RecvStream)> {
    conn.accept_bi().await.map_err(map_quic_err)
}

/// Open a new bidirectional stream on a QUIC connection.
pub async fn open_bi(conn: &Connection) -> Result<(SendStream, RecvStream)> {
    conn.open_bi().await.map_err(map_quic_err)
}

fn map_err<E: std::fmt::Display>(err: E) -> Error {
    Error::Protocol(format!("p2p: {err}"))
}

fn map_io_err<E: std::fmt::Display>(err: E) -> Error {
    Error::Protocol(format!("p2p io: {err}"))
}

fn map_quic_err<E: std::fmt::Display>(err: E) -> Error {
    Error::Protocol(format!("p2p quic: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn ticket_round_trip_relay_and_direct() {
        // Build a ticket manually to avoid needing a real endpoint in unit tests.
        let id = EndpointId::from_str(
            "ce6db6f1d5b69cf1cb98ddc06f25f1bc7eb3a8b5a3a7e9b96fa6e3d2cc04a7f8",
        )
        .unwrap();
        let ticket = PeerTicket {
            endpoint_id: id,
            relay: Some(RelayUrl::from_str("https://relay.example/").unwrap()),
            direct: vec![std::net::SocketAddr::from((
                Ipv4Addr::new(192, 168, 1, 5),
                12345,
            ))],
        };
        let encoded = ticket.encode();
        let decoded = PeerTicket::decode(&encoded).expect("decode");
        assert_eq!(decoded.endpoint_id, ticket.endpoint_id);
        assert_eq!(
            decoded.relay.as_ref().map(|u| u.as_str().to_string()),
            ticket.relay.as_ref().map(|u| u.as_str().to_string())
        );
        assert_eq!(decoded.direct, ticket.direct);
    }

    #[test]
    fn ticket_decode_rejects_bad_prefix() {
        assert!(PeerTicket::decode("http://wrong/").is_err());
    }
}
