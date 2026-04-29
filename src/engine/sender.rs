//! Sender side of the protocol.

use std::net::{SocketAddr, TcpListener, TcpStream};

use indicatif::{ProgressBar, ProgressStyle};
use tracing::info;

use crate::craft::CraftFile;
use crate::engine::handshake::{local_hello, ProtocolMessage, PROTOCOL_VERSION};
use crate::engine::DEFAULT_CHUNK_BYTES;
use crate::transport::tcp::{recv_frame, send_frame, tune};
use crate::{Error, Result};

/// Where the sender should accept (or initiate) the connection.
#[derive(Debug, Clone)]
pub struct SendOptions {
    /// Address to bind on when running as a passive sender.
    pub bind: SocketAddr,
    /// When set, actively connect to this address instead of binding.
    pub connect_to: Option<SocketAddr>,
}

/// Send a single craft file to one connecting/connected peer.
pub fn send_blueprint(craft: &CraftFile, opts: &SendOptions) -> Result<()> {
    let mut stream = match opts.connect_to {
        Some(addr) => {
            info!(target: "ksp_share::send", "Connecting to {addr}");
            TcpStream::connect(addr)?
        }
        None => {
            let listener = TcpListener::bind(opts.bind)?;
            let local = listener.local_addr()?;
            println!("Listening on {local} — share this address with the receiver");
            let (stream, peer) = listener.accept()?;
            info!(target: "ksp_share::send", "Accepted connection from {peer}");
            stream
        }
    };
    tune(&stream)?;
    run_session(&mut stream, craft)
}

fn run_session(stream: &mut TcpStream, craft: &CraftFile) -> Result<()> {
    send_frame(stream, &local_hello())?;
    let peer_hello: ProtocolMessage = recv_frame(stream)?;
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

    send_frame(stream, &ProtocolMessage::Meta(craft.metadata.clone()))?;
    match recv_frame::<ProtocolMessage, _>(stream)? {
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
            stream,
            &ProtocolMessage::Data {
                offset,
                bytes: chunk.to_vec(),
            },
        )?;
        offset += chunk.len() as u64;
        progress.set_position(offset);
    }
    send_frame(stream, &ProtocolMessage::Eof)?;
    progress.finish_with_message("done");

    match recv_frame::<ProtocolMessage, _>(stream)? {
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
    send_frame(stream, &ProtocolMessage::Done)?;
    println!(
        "✓ Sent \"{}\" ({} bytes) to peer",
        craft.metadata.name, craft.metadata.size_bytes
    );
    Ok(())
}
