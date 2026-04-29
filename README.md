# KSP BlueprintShare (`ksp-share`)

> **Open-Source · MIT · Cross-Platform CLI**
> Send and receive Kerbal Space Program `.craft` blueprints between
> friends with a single command.

```
Sender:    ksp-share send "Mun Rocket III"
Receiver:  ksp-share receive --from <ip:port>
→ blueprint lands in the receiver's KSP Ships/VAB/ folder.
```

No accounts, no cloud, no upload step — the file goes straight from one
machine to another over TCP, with a SHA-256 integrity check at the end.

This repository implements **stages 0–3a** of the
[project plan](./docs/protocol.md): bootstrap, TCP MVP, KSP integration
and LAN auto-discovery via mDNS. Stages 3b+ (QUIC P2P with relay
fallback, GUI) are tracked in the [Roadmap](#roadmap) below.

## Status

[![Build](https://github.com/LevaKost/KSP-BC/actions/workflows/build.yml/badge.svg)](https://github.com/LevaKost/KSP-BC/actions/workflows/build.yml)

- [x] Cargo workspace with `ksp-share` binary and library
- [x] TCP transport with length-prefixed `bincode` framing
- [x] `HELLO → META → READY → DATA → EOF → VERIFY → DONE` protocol
- [x] SHA-256 verification end-to-end with progress bar
- [x] KSP install auto-detection (Steam + sensible fallbacks)
- [x] `.craft` metadata parser for KSP1 and KSP2
- [x] CI + cross-platform release pipeline
- [x] LAN mDNS auto-discovery (stage 3a)
- [ ] QUIC + relay P2P transport (stage 3b)
- [ ] GUI (stage 4)

## Install

### Pre-built binaries

Each tagged release publishes binaries for Windows, macOS (x86_64 +
arm64) and Linux on the
[Releases page](https://github.com/LevaKost/KSP-BC/releases).
Each binary ships with a `.sha256` companion file.

### Build from source

```sh
git clone https://github.com/LevaKost/KSP-BC
cd KSP-BC
cargo build --release
./target/release/ksp-share --help
```

See [`docs/build-from-source.md`](./docs/build-from-source.md) for
toolchain details.

## Usage

### Sender

```sh
# By blueprint name (resolved against the detected KSP install).
ksp-share send "Mun Rocket III"

# Or by file path.
ksp-share send /path/to/MyRocket.craft

# Active sender (dial out to a known receiver).
ksp-share send "Mun Rocket III" --to 192.168.1.5:7878
```

By default `ksp-share send` binds `0.0.0.0:7878`, **publishes a
`_ksp-share._tcp.local.` mDNS record on the LAN** and waits for the
receiver to connect. Pass `--no-mdns` to suppress the announcement.

### Receiver

```sh
# LAN auto-discovery (default): browse for a sender via mDNS and dial it.
ksp-share receive

# Connect to a sender that's listening at a specific address.
ksp-share receive --from 192.168.1.5:7878

# Or wait for the sender to dial in.
ksp-share receive --bind 0.0.0.0:7878

# Skip the accept prompt and pin the destination directory.
ksp-share receive --from 192.168.1.5:7878 --out ./inbox --yes
```

Received files are placed under `<KSP>/Ships/VAB/` or
`<KSP>/Ships/SPH/` depending on the blueprint type. Override the
destination with `--out`.

### Listing local blueprints

```sh
ksp-share list
ksp-share list --ship vab
```

### Discovering peers on the LAN

```sh
# Browse for active senders for 5s and print what's announced.
ksp-share discover

# Watch continuously (Ctrl-C to stop).
ksp-share discover --timeout 0
```

See [`docs/discovery.md`](./docs/discovery.md) for the service type,
TXT record schema and tips on running mDNS through firewalls.

### Inspecting the detected install

```sh
ksp-share config
```

Set the `KSP_ROOT` environment variable to override detection.

## Roadmap

| Stage | Scope                                              | Status      |
|-------|----------------------------------------------------|-------------|
| 0     | Bootstrap, license, README, CI                     | ✅ done      |
| 1     | TCP MVP, SHA-256, progress bar                     | ✅ done      |
| 2     | KSP install detector, `list`, send-by-name         | ✅ done      |
| 3a    | LAN auto-discovery via mDNS                        | ✅ done      |
| 3b    | QUIC P2P, relay fallback                           | 🟡 planned  |
| 4     | GUI (`egui`/`eframe`), drag-and-drop               | 🟡 planned  |

## Contributing

1. Fork the repository.
2. `git checkout -b feature/short-name`.
3. Add tests for new code.
4. `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.
5. Open a PR against `main`.

### Code conventions

- `cargo fmt` is mandatory.
- Errors via `thiserror`, logs via `tracing`.
- No `unwrap()` in production code — use `?` or an explicit `expect`.
- Public APIs are documented with `///` doc-comments.

## License

[MIT](./LICENSE) © LevaKost and contributors.

> Made for the KSP community. No servers, no signups — just rockets.
