# BlueprintShare wire protocol

The MVP transport is a single TCP stream framed as
`u32_be length + bincode(ProtocolMessage)`. Frames are capped at **8 MiB**
per message to bound peer memory.

The same message enum will be reused once the QUIC transport lands; only
the framing layer changes.

## Sequence diagram

```
Sender                              Receiver
  │                                    │
  │──── HELLO {version, platform} ────►│
  │◄─── HELLO {version, platform} ─────│
  │                                    │
  │──── META {                         │
  │        name, size_bytes,           │
  │        sha256, ksp_version,        │
  │        ship_type, generation       │
  │     } ────────────────────────────►│
  │◄─── READY {accept, reason?} ───────│
  │                                    │
  │──── DATA {offset, bytes} (×N) ────►│
  │──── EOF ──────────────────────────►│
  │                                    │
  │◄─── VERIFY {sha256_ok} ────────────│
  │──── DONE ─────────────────────────►│
```

## Message variants

```rust
pub enum ProtocolMessage {
    Hello { version: u16, platform: String, client: String },
    Ready { accept: bool, reason: Option<String> },
    Meta(CraftMetadata),
    Ack,
    Data { offset: u64, bytes: Vec<u8> },
    Eof,
    Verify { sha256_ok: bool },
    Done,
    Error { message: String },
}
```

`CraftMetadata` carries: `name`, `size_bytes`, `sha256`, `ksp_version`,
`ship_type` (`Vab | Sph | Unknown`) and `generation` (`Ksp1 | Ksp2 |
Unknown`).

## Versioning

`PROTOCOL_VERSION` lives in `src/engine/handshake.rs`. Both peers exchange
`HELLO` first; if the versions disagree the receiver answers with `Error`
and both sides terminate.

## Chunking

`DATA` frames carry **64 KiB** chunks by default
(`engine::DEFAULT_CHUNK_BYTES`). Receivers track the cumulative offset and
fail fast on any out-of-order chunk.

## Integrity

The sender computes a SHA-256 of the file and sends it in `META`. The
receiver streams chunks into memory, recomputes the digest after `EOF` and
replies with `VERIFY { sha256_ok }`. On mismatch the receiver does **not**
write the file to disk.
