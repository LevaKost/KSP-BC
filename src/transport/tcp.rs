//! Length-prefixed TCP transport for the BlueprintShare protocol.
//!
//! Each frame on the wire is `u32_be length` followed by `length` bytes
//! of `bincode`-encoded payload. The framing is intentionally simple so
//! the same wire format can be reused once a QUIC stream is plumbed in.

use std::io::{Read, Write};
use std::net::TcpStream;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{Error, Result};

/// Hard ceiling on a single framed message (8 MiB) to bound memory.
const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

/// Encode and send a value as a length-prefixed bincode frame.
pub fn send_frame<S: Serialize, W: Write>(writer: &mut W, value: &S) -> Result<()> {
    let payload = bincode::serialize(value)?;
    if payload.len() as u64 > MAX_FRAME_BYTES as u64 {
        return Err(Error::TooLarge {
            size: payload.len() as u64,
            limit: MAX_FRAME_BYTES as u64,
        });
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    Ok(())
}

/// Receive and decode a length-prefixed bincode frame.
pub fn recv_frame<D: DeserializeOwned, R: Read>(reader: &mut R) -> Result<D> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        return Err(Error::TooLarge {
            size: len as u64,
            limit: MAX_FRAME_BYTES as u64,
        });
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    let value = bincode::deserialize(&buf)?;
    Ok(value)
}

/// Tunable TCP socket options applied to both peers.
pub fn tune(stream: &TcpStream) -> Result<()> {
    stream.set_nodelay(true)?;
    Ok(())
}
