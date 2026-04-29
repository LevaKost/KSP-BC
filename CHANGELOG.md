# Changelog

All notable changes to this project will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- QUIC P2P transport via [iroh](https://crates.io/crates/iroh), behind
  the optional `p2p` Cargo feature (default-off to keep build size
  small).
  - `ksp-share send --p2p` prints a peer ticket
    (`ksp-share://<endpoint_id>?relay=...&direct=...`) and waits for
    an inbound connection.
  - `ksp-share receive --ticket <ticket>` dials it.
  - NAT hole-punching is handled transparently; if direct UDP fails,
    the connection seamlessly falls back to a public relay server.
  - End-to-end encrypted via QUIC + TLS 1.3 with key pinning to the
    sender's endpoint id.
  - New module `src/transport/p2p.rs` (ALPN, ticket encode/decode,
    framed bincode over QUIC streams).
  - New module `src/engine/quic.rs` mirrors the existing TCP
    sender/receiver flows on tokio + iroh.
  - New `tests/quic_transfer.rs` smoke test (round-trips a craft file
    between two iroh endpoints in the same process). Marked
    `#[ignore]` because iroh's default preset reaches public relay
    servers; run locally with
    `cargo test --features p2p --test quic_transfer -- --ignored`.
  - New CI job `test-p2p` builds and tests the feature on Linux.
  - `docs/p2p.md` documents the ticket format, threat model and
    relay-server caveats.
- LAN auto-discovery via mDNS (`_ksp-share._tcp.local.`).
  - `ksp-share send` publishes a service record with the blueprint name,
    size, ship type, KSP version and protocol version in TXT records.
  - `ksp-share receive` browses the LAN by default and dials the first
    matching sender (or presents a picker when several are visible).
  - New `ksp-share discover` subcommand prints active senders.
  - New `--no-mdns` flag on both `send` and `receive` to opt out.
- New `tests/mdns_discovery.rs` smoke test (round-trips an announcement
  through the local mDNS daemon; marked `#[ignore]` because some CI
  runners block multicast).
- `docs/discovery.md` documenting the service type, TXT schema and
  networking caveats.

### Changed
- `SendOptions` is now an enum (`Connect(SocketAddr) | Listen(TcpListener)`)
  so the CLI can bind the listener up front and announce its **actual**
  port (matters when binding to `:0`).
- `send_blueprint` now takes `SendOptions` by value.

### Fixed
- Receiver no longer truncates blueprint names that contain dots (e.g.
  `Rocket v2.0.craft`).

## [0.1.0-alpha.1] — initial bootstrap

### Added
- Initial Cargo workspace bootstrap (`ksp-share` binary + `ksp_blueprintshare` library).
- TCP MVP transport with length-prefixed `bincode` framing.
- Protocol messages for the `HELLO → READY → META → ACK → DATA → EOF → VERIFY → DONE` flow.
- `clap`-based CLI: `send`, `receive`, `list`, `config`.
- KSP install detector for Linux, macOS and Windows (Steam + sensible
  fallbacks, plus a `KSP_ROOT` override).
- Lightweight `.craft` metadata parser for both KSP1 and KSP2 layouts.
- SHA-256 verification end-to-end with a progress bar via `indicatif`.
- GitHub Actions: `build.yml` (fmt + clippy + tests on Linux/macOS/Windows)
  and `release.yml` (cross-platform binaries on `v*.*.*` tags).
- Docs: `docs/protocol.md`, `docs/ksp-paths.md`, `docs/build-from-source.md`.

[Unreleased]: https://github.com/LevaKost/KSP-BC/compare/HEAD...HEAD
