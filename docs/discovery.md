# LAN auto-discovery (mDNS / DNS-SD)

`ksp-share send` publishes a Multicast DNS service so that
`ksp-share receive` (and `ksp-share discover`) can find it on the same
broadcast domain without the user having to type an IP address.

The implementation is built on top of the existing TCP transport — no
new wire protocol is introduced. Discovery is purely about *finding*
the sender's address; the bytes still flow over the same length-prefixed
bincode framing described in [`protocol.md`](./protocol.md).

## Service type

```
_ksp-share._tcp.local.
```

The instance name is the blueprint's friendly name as parsed from the
`.craft` file (e.g. `Mun Rocket III._ksp-share._tcp.local.`). The
underlying mDNS hostname is derived from the blueprint name and is
unique per share.

## TXT record schema

| Key       | Type   | Description                                                   |
|-----------|--------|---------------------------------------------------------------|
| `name`    | string | Human-readable blueprint name (also encoded in the instance). |
| `version` | u16    | Wire protocol version the sender speaks (`PROTOCOL_VERSION`). |
| `size`    | u64    | Blueprint payload size in bytes.                              |
| `ship`    | string | One of `VAB`, `SPH`, `Ship`.                                  |
| `kspver`  | string | Optional — the KSP version reported by the `.craft` header.   |

Receivers use these fields to render a friendly picker (`name`, `size`,
`ship`) before dialing the resolved address. `version` lets the
receiver bail out early if the sender speaks a newer protocol it can't
talk to.

## Sender behaviour

By default, `ksp-share send <blueprint>` binds the listener, registers
the service, prints

```
Listening on 0.0.0.0:7878 — share this address with the receiver
Announcing on LAN as `Mun Rocket III` (mDNS service `_ksp-share._tcp.local.`, port 7878)
```

and waits for the receiver to connect. The handle is tied to the
process — when the sender exits, the announcement is unregistered
explicitly and the daemon shuts down (RAII via
`AnnouncementHandle::drop`).

Pass `--no-mdns` to suppress the announcement entirely (e.g. when
running on networks where multicast is blocked or undesirable).

## Receiver behaviour

`ksp-share receive` resolves the connection like this:

1. `--from <addr>` → dial out, no discovery.
2. `--bind <addr>` → bind & wait, no discovery.
3. otherwise → browse the LAN via mDNS for `--discover-timeout` seconds
   (default 4s):
   - exactly one sender → dial it automatically
   - multiple senders → present a numbered picker
   - none → fall back to binding on the default port and waiting

Pass `--no-mdns` to skip step 3 entirely.

## Standalone discovery

```
ksp-share discover               # 5s scan
ksp-share discover --timeout 0   # run until Ctrl-C, print as found
```

Useful for verifying that a sender on another machine is visible from
yours.

## Networking notes

mDNS uses UDP port 5353 on the multicast group 224.0.0.251 (IPv4) and
ff02::fb (IPv6). It only works inside a single broadcast domain — most
home networks are a single VLAN, so this Just Works. If discovery fails:

- check that both peers are on the same Wi-Fi/Ethernet segment;
- some "guest" Wi-Fi networks isolate clients from each other and will
  block mDNS (and direct TCP) — switch to the main network or fall
  back to `--from <addr>`;
- corporate firewalls and some VPNs strip multicast — disable the VPN
  on the LAN, or pass an explicit `--from`/`--to` address.

The browser uses [`mdns-sd`](https://crates.io/crates/mdns-sd), which
is a pure-Rust implementation with no dependency on Avahi, Bonjour or
any system service — `ksp-share` works out of the box on Linux, macOS
and Windows.
