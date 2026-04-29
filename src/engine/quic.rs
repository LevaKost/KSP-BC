//! QUIC-based send/receive flows. Mirror the synchronous TCP variants
//! one-to-one but run on tokio and use iroh's QUIC streams under the
//! hood. Only compiled when the `p2p` Cargo feature is enabled.
//!
//! The wire-level protocol is identical to the TCP version (see
//! `docs/protocol.md`); the only role swap is that, on QUIC, the side
//! that *opens* the bidirectional stream has to write the first frame
//! (iroh streams are accepted lazily on first byte). Our sender always
//! accepts, so the receiver is the connector and it sends `HELLO`
//! before the sender does.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use indicatif::{ProgressBar, ProgressStyle};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr};
use tracing::info;

use crate::craft::{sha256_hex, CraftFile, CraftMetadata};
use crate::engine::handshake::{local_hello, ProtocolMessage, PROTOCOL_VERSION};
use crate::engine::DEFAULT_CHUNK_BYTES;
use crate::ksp::{KspInstall, ShipType};
use crate::transport::p2p::{accept_bi, bind_endpoint, open_bi, recv_frame, send_frame, ALPN};
use crate::{Error, Result};

/// Receiver-side configuration for a QUIC transfer.
#[derive(Debug, Clone)]
pub struct QuicReceiveOptions {
    /// Optional override for the destination directory.
    pub output_dir: Option<PathBuf>,
    /// Detected install. Used when `output_dir` is absent.
    pub ksp_install: Option<KspInstall>,
    /// Skip the interactive accept prompt.
    pub auto_accept: bool,
}

/// Bind an iroh endpoint and return it together with its publishable
/// [`EndpointAddr`]. Waits for the endpoint to be online so the
/// returned address is suitable for printing as a ticket.
pub async fn bind_p2p() -> Result<(Endpoint, EndpointAddr)> {
    let endpoint = bind_endpoint().await?;
    endpoint.online().await;
    let addr = endpoint.addr();
    Ok((endpoint, addr))
}

/// Bind a *receiver-side* endpoint that does not need to wait to be
/// online before connecting (we are the connector, not the listener).
pub async fn bind_p2p_dialer() -> Result<Endpoint> {
    bind_endpoint().await
}

/// Sender flow. Accepts a single inbound iroh connection on the
/// supplied endpoint and pushes a craft file to it. Returns when the
/// transfer finishes or the peer hangs up.
pub async fn send_blueprint_quic(endpoint: &Endpoint, craft: &CraftFile) -> Result<()> {
    let incoming = endpoint
        .accept()
        .await
        .ok_or_else(|| Error::Protocol("p2p: endpoint stopped accepting connections".into()))?;
    let conn: Connection = incoming
        .await
        .map_err(|err| Error::Protocol(format!("p2p connecting: {err}")))?;
    info!(target: "ksp_share::send", "Accepted iroh connection from {}", conn.remote_id());
    let (mut send, mut recv) = accept_bi(&conn).await?;
    run_sender_session(&mut send, &mut recv, craft).await?;
    // Make sure all queued frames are flushed before closing.
    send.finish().ok();
    conn.closed().await;
    Ok(())
}

/// Receiver flow. Connects to `peer` via iroh and pulls the craft.
pub async fn receive_blueprint_quic(
    endpoint: &Endpoint,
    peer: EndpointAddr,
    opts: &QuicReceiveOptions,
) -> Result<()> {
    info!(target: "ksp_share::recv", "Dialing iroh peer {}", peer.id);
    let conn = endpoint
        .connect(peer, ALPN)
        .await
        .map_err(|err| Error::Protocol(format!("p2p connect: {err}")))?;
    let (mut send, mut recv) = open_bi(&conn).await?;
    run_receiver_session(&mut send, &mut recv, opts).await?;
    send.finish().ok();
    conn.close(0u32.into(), b"done");
    Ok(())
}

async fn run_sender_session(
    send: &mut SendStream,
    recv: &mut RecvStream,
    craft: &CraftFile,
) -> Result<()> {
    // On QUIC the connector (= receiver) writes first, so we read the
    // peer HELLO before sending ours.
    let peer_hello: ProtocolMessage = recv_frame(recv).await?;
    let peer_version = match peer_hello {
        ProtocolMessage::Hello { version, .. } => version,
        other => {
            return Err(Error::Protocol(format!(
                "expected HELLO, got {}",
                other.kind()
            )))
        }
    };
    if peer_version != PROTOCOL_VERSION {
        return Err(Error::VersionMismatch {
            peer: peer_version,
            ours: PROTOCOL_VERSION,
        });
    }
    send_frame(send, &local_hello()).await?;

    send_frame(send, &ProtocolMessage::Meta(craft.metadata.clone())).await?;
    match recv_frame::<ProtocolMessage>(recv).await? {
        ProtocolMessage::Ready { accept: true, .. } => {}
        ProtocolMessage::Ready {
            accept: false,
            reason,
        } => {
            return Err(Error::PeerRejected {
                reason: reason.unwrap_or_else(|| "no reason given".into()),
            });
        }
        other => {
            return Err(Error::Protocol(format!(
                "expected READY, got {}",
                other.kind()
            )));
        }
    }

    let progress = ProgressBar::new(craft.metadata.size_bytes);
    progress.set_style(
        ProgressStyle::with_template(
            "{prefix:>10} {bar:30.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    progress.set_prefix("send");

    let mut offset: u64 = 0;
    for chunk in craft.bytes.chunks(DEFAULT_CHUNK_BYTES) {
        send_frame(
            send,
            &ProtocolMessage::Data {
                offset,
                bytes: chunk.to_vec(),
            },
        )
        .await?;
        offset += chunk.len() as u64;
        progress.set_position(offset);
    }
    send_frame(send, &ProtocolMessage::Eof).await?;
    progress.finish_with_message("done");

    match recv_frame::<ProtocolMessage>(recv).await? {
        ProtocolMessage::Verify { sha256_ok: true } => {}
        ProtocolMessage::Verify { sha256_ok: false } => {
            return Err(Error::ChecksumMismatch {
                expected: craft.metadata.sha256.clone(),
                actual: "<peer reported mismatch>".into(),
            });
        }
        other => {
            return Err(Error::Protocol(format!(
                "expected VERIFY, got {}",
                other.kind()
            )));
        }
    }
    send_frame(send, &ProtocolMessage::Done).await?;
    println!(
        "✓ Sent \"{}\" ({} bytes) over iroh QUIC",
        craft.metadata.name, craft.metadata.size_bytes
    );
    Ok(())
}

async fn run_receiver_session(
    send: &mut SendStream,
    recv: &mut RecvStream,
    opts: &QuicReceiveOptions,
) -> Result<()> {
    // Receiver opens the stream and therefore writes first.
    send_frame(send, &local_hello()).await?;
    let peer_hello: ProtocolMessage = recv_frame(recv).await?;
    let peer_version = match peer_hello {
        ProtocolMessage::Hello { version, .. } => version,
        other => {
            return Err(Error::Protocol(format!(
                "expected HELLO, got {}",
                other.kind()
            )))
        }
    };
    if peer_version != PROTOCOL_VERSION {
        send_frame(
            send,
            &ProtocolMessage::Error {
                message: format!("incompatible protocol version {peer_version}"),
            },
        )
        .await?;
        return Err(Error::VersionMismatch {
            peer: peer_version,
            ours: PROTOCOL_VERSION,
        });
    }

    let meta = match recv_frame::<ProtocolMessage>(recv).await? {
        ProtocolMessage::Meta(m) => m,
        other => {
            return Err(Error::Protocol(format!(
                "expected META, got {}",
                other.kind()
            )))
        }
    };

    if !accept_transfer(&meta, opts)? {
        send_frame(
            send,
            &ProtocolMessage::Ready {
                accept: false,
                reason: Some("user declined".into()),
            },
        )
        .await?;
        return Err(Error::PeerRejected {
            reason: "user declined".into(),
        });
    }
    send_frame(
        send,
        &ProtocolMessage::Ready {
            accept: true,
            reason: None,
        },
    )
    .await?;

    let progress = ProgressBar::new(meta.size_bytes);
    progress.set_style(
        ProgressStyle::with_template(
            "{prefix:>10} {bar:30.green/blue} {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    progress.set_prefix("recv");

    let mut buffer: Vec<u8> = Vec::with_capacity(meta.size_bytes as usize);
    loop {
        match recv_frame::<ProtocolMessage>(recv).await? {
            ProtocolMessage::Data { offset, bytes } => {
                if offset != buffer.len() as u64 {
                    return Err(Error::Protocol(format!(
                        "out of order DATA chunk: got offset {offset}, expected {}",
                        buffer.len()
                    )));
                }
                buffer.extend_from_slice(&bytes);
                progress.set_position(buffer.len() as u64);
            }
            ProtocolMessage::Eof => break,
            other => {
                return Err(Error::Protocol(format!(
                    "expected DATA/EOF, got {}",
                    other.kind()
                )));
            }
        }
    }
    progress.finish_with_message("done");

    let actual = sha256_hex(&buffer);
    let ok = actual == meta.sha256;
    send_frame(send, &ProtocolMessage::Verify { sha256_ok: ok }).await?;
    if !ok {
        return Err(Error::ChecksumMismatch {
            expected: meta.sha256.clone(),
            actual,
        });
    }
    match recv_frame::<ProtocolMessage>(recv).await? {
        ProtocolMessage::Done => {}
        other => {
            return Err(Error::Protocol(format!(
                "expected DONE, got {}",
                other.kind()
            )));
        }
    }

    let target = resolve_target_path(&meta, opts)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&target)?;
    file.write_all(&buffer)?;
    println!(
        "✓ Saved \"{}\" ({} bytes) to {} — SHA-256 OK",
        meta.name,
        meta.size_bytes,
        target.display()
    );
    Ok(())
}

fn accept_transfer(meta: &CraftMetadata, opts: &QuicReceiveOptions) -> Result<bool> {
    if opts.auto_accept {
        return Ok(true);
    }
    println!(
        "Incoming blueprint \"{}\" ({} bytes) [{}]. Accept? [Y/n]",
        meta.name,
        meta.size_bytes,
        match meta.ship_type {
            ShipType::Vab => "VAB",
            ShipType::Sph => "SPH",
            ShipType::Unknown => "?",
        }
    );
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    Ok(answer.is_empty() || answer == "y" || answer == "yes")
}

fn resolve_target_path(meta: &CraftMetadata, opts: &QuicReceiveOptions) -> Result<PathBuf> {
    let mut dir = if let Some(custom) = opts.output_dir.clone() {
        custom
    } else {
        let install = opts.ksp_install.as_ref().ok_or(Error::KspNotFound)?;
        match meta.ship_type {
            ShipType::Sph => install.sph_dir(),
            _ => install.vab_dir(),
        }
    };
    dir.push(format!("{}.craft", sanitize(&meta.name)));
    Ok(dir)
}

fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(
            ch,
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
        ) {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "blueprint".into()
    } else {
        trimmed.to_string()
    }
}
