//! Receiver side of the protocol.

use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;

use indicatif::{ProgressBar, ProgressStyle};
use tracing::info;

use crate::craft::{sha256_hex, CraftMetadata};
use crate::engine::handshake::{local_hello, ProtocolMessage, PROTOCOL_VERSION};
use crate::ksp::{KspInstall, ShipType};
use crate::transport::tcp::{recv_frame, send_frame, tune};
use crate::{Error, Result};

/// Connection / output settings for the receiver.
#[derive(Debug, Clone)]
pub struct ReceiveOptions {
    /// When set, dial out to this address.
    pub connect_to: Option<SocketAddr>,
    /// Bind here when [`Self::listen`] is true.
    pub bind: SocketAddr,
    /// Whether to wait for an inbound connection.
    pub listen: bool,
    /// Optional override for the destination directory.
    pub output_dir: Option<PathBuf>,
    /// Detected install. Used when `output_dir` is absent.
    pub ksp_install: Option<KspInstall>,
    /// Skip the interactive accept prompt.
    pub auto_accept: bool,
}

/// Run a single receive session and place the resulting file under the
/// detected KSP install (or a user-supplied directory).
pub fn receive_blueprint(opts: &ReceiveOptions) -> Result<()> {
    let mut stream = match (opts.listen, opts.connect_to) {
        (false, Some(addr)) => {
            info!(target: "ksp_share::recv", "Connecting to {addr}");
            TcpStream::connect(addr)?
        }
        (true, _) => {
            let listener = TcpListener::bind(opts.bind)?;
            let local = listener.local_addr()?;
            println!("Listening on {local} — share this address with the sender");
            let (stream, peer) = listener.accept()?;
            info!(target: "ksp_share::recv", "Accepted connection from {peer}");
            stream
        }
        (false, None) => {
            return Err(Error::Protocol(
                "receiver needs either --from <addr> or --bind".into(),
            ));
        }
    };
    tune(&stream)?;
    run_session(&mut stream, opts)
}

fn run_session(stream: &mut TcpStream, opts: &ReceiveOptions) -> Result<()> {
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
        send_frame(
            stream,
            &ProtocolMessage::Error {
                message: format!("incompatible protocol version {peer_version}"),
            },
        )?;
        return Err(Error::VersionMismatch {
            peer: peer_version,
            ours: PROTOCOL_VERSION,
        });
    }
    send_frame(stream, &local_hello())?;

    let meta = match recv_frame::<ProtocolMessage, _>(stream)? {
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
            stream,
            &ProtocolMessage::Ready {
                accept: false,
                reason: Some("user declined".into()),
            },
        )?;
        return Err(Error::PeerRejected {
            reason: "user declined".into(),
        });
    }
    send_frame(
        stream,
        &ProtocolMessage::Ready {
            accept: true,
            reason: None,
        },
    )?;

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
        match recv_frame::<ProtocolMessage, _>(stream)? {
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
    send_frame(stream, &ProtocolMessage::Verify { sha256_ok: ok })?;
    if !ok {
        return Err(Error::ChecksumMismatch {
            expected: meta.sha256.clone(),
            actual,
        });
    }
    match recv_frame::<ProtocolMessage, _>(stream)? {
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

fn accept_transfer(meta: &CraftMetadata, opts: &ReceiveOptions) -> Result<bool> {
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

fn resolve_target_path(meta: &CraftMetadata, opts: &ReceiveOptions) -> Result<PathBuf> {
    let mut dir = if let Some(custom) = opts.output_dir.clone() {
        custom
    } else {
        let install = opts.ksp_install.as_ref().ok_or(Error::KspNotFound)?;
        match meta.ship_type {
            ShipType::Sph => install.sph_dir(),
            _ => install.vab_dir(),
        }
    };
    // Build the filename manually instead of using `set_extension`, which
    // truncates everything after the last `.` in the stem (so a blueprint
    // called "Rocket v2.0" would otherwise land as "Rocket v2.craft").
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
