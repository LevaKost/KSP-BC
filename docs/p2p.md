# P2P transport (QUIC + relay)

`ksp-share` ships an optional QUIC transport built on
[iroh](https://crates.io/crates/iroh) for situations where the LAN
discovery story isn't enough — typically when the two peers are on
different networks behind NATs.

When the binary is built with `--features p2p`, two new flows become
available:

- `ksp-share send <blueprint> --p2p` binds an iroh endpoint, prints a
  ticket, and waits for an inbound connection.
- `ksp-share receive --ticket <ticket>` connects to the sender via
  iroh.

The on-the-wire protocol is the same `HELLO → META → READY → DATA →
EOF → VERIFY → DONE` flow described in [`protocol.md`](./protocol.md);
only the transport changes.

## How it works

- Each iroh endpoint is identified by an Ed25519 public key (its
  `EndpointId`). Tickets carry the id plus optional direct UDP
  addresses and a relay URL.
- iroh first attempts a direct UDP connection (with hole-punching).
- If hole-punching fails, the connection automatically falls back to a
  public **relay server** — packets are forwarded through the relay
  but stay end-to-end encrypted under the QUIC TLS session.
- The relay is only used as a fallback; once a direct path is
  available iroh transparently switches to it.

## Ticket format

The CLI prints a single line of the form

```
ksp-share://<endpoint_id>?relay=<url>&direct=<host:port>&direct=<host:port>...
```

This is a tiny, human-readable wrapper around an iroh `EndpointAddr`.
You can paste it into chat / Discord / a QR code generator and it will
round-trip cleanly.

Field *values* (the relay URL and each direct address) are
percent-encoded so that relay URLs containing reserved query
characters (`&`, `=`, `?`, `#`, `+`, space) survive the round-trip.
The unreserved set is `[A-Za-z0-9-._~:/]` per RFC 3986; everything
else is `%XX`-escaped.

## Building with the P2P feature

```sh
cargo build --release --features p2p
```

This pulls in iroh (~200 crates) and a tokio runtime, so the first
build takes a few minutes. Subsequent builds are incremental.

The pre-built binaries on the
[releases page](https://github.com/LevaKost/KSP-BC/releases) ship the
default feature set only — TCP + LAN mDNS. To get QUIC support, build
from source as above (or grab a `-p2p`-suffixed release once we start
publishing them).

## Threat model & privacy notes

- Tickets contain a public key but not your IP address until the
  relay server hands one out, and direct addresses are only included
  if you've already discovered them. Pasting a ticket into a public
  chat is roughly comparable to sharing a phone number — anyone with
  it can attempt to dial you, but they can't impersonate you.
- All traffic is end-to-end encrypted via QUIC + TLS 1.3 with cert
  pinning to the endpoint key. Relay operators see ciphertext only.
- The relay servers are operated by [number 0](https://n0.computer/)
  by default. To pin to your own infrastructure, future versions will
  expose `--relay-url`; today the iroh defaults are baked in.

## Manual testing

End-to-end smoke test on a single machine (replace the ticket between
runs):

```
# Terminal 1
$ ksp-share send "Mun Rocket III" --p2p
iroh endpoint online — share this ticket with the receiver:
  ksp-share://...?relay=https://...&direct=192.168.1.5:54321
Waiting for an inbound connection (Ctrl-C to cancel)…

# Terminal 2
$ ksp-share receive --ticket 'ksp-share://...?relay=...' --out ./inbox --yes
```

The integration test `tests/quic_transfer.rs` exercises the same flow
in-process; run it with

```
cargo test --features p2p --test quic_transfer -- --ignored
```
