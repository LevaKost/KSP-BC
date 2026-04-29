//! LAN auto-discovery via Multicast DNS (RFC 6762 / 6763).
//!
//! The sender publishes a `_ksp-share._tcp.local.` service announcing
//! the blueprint name, protocol version and listening port; receivers
//! browse the same service type and pick one of the resolved peers.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use mdns_sd::{ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo};
// Used: ServiceDaemon, ServiceEvent::ServiceResolved, ServiceInfo (for builder),
// ResolvedService (for the resolved event payload), ScopedIp (for address extraction).

use crate::engine::PROTOCOL_VERSION;
use crate::{Error, Result};

/// DNS-SD service type for `ksp-share`.
pub const SERVICE_TYPE: &str = "_ksp-share._tcp.local.";

/// TXT record key carrying the blueprint name.
pub const TXT_NAME: &str = "name";
/// TXT record key carrying the wire protocol version.
pub const TXT_VERSION: &str = "version";
/// TXT record key carrying the blueprint payload size in bytes.
pub const TXT_SIZE: &str = "size";
/// TXT record key carrying the originating KSP version.
pub const TXT_KSP_VERSION: &str = "kspver";
/// TXT record key carrying the ship type (`vab`, `sph` or `unknown`).
pub const TXT_SHIP: &str = "ship";

/// Information published by a sender on the LAN.
#[derive(Debug, Clone)]
pub struct AnnouncedShare {
    /// `<blueprint>._ksp-share._tcp.local.` — usable with `unregister`.
    pub fullname: String,
    /// Blueprint name advertised by the sender, if any.
    pub blueprint: Option<String>,
    /// Wire protocol version reported by the sender.
    pub protocol_version: Option<u16>,
    /// Reported blueprint size in bytes, if any.
    pub size_bytes: Option<u64>,
    /// Reported ship type, if any.
    pub ship_type: Option<String>,
    /// Reported KSP game version, if any.
    pub ksp_version: Option<String>,
    /// Resolved socket address (first IPv4 address ∪ port).
    pub addr: SocketAddr,
}

/// Wrap a [`ServiceDaemon`] handle and unregister the service when
/// dropped, so callers can rely on RAII to take the announcement off
/// the air on early exit.
pub struct AnnouncementHandle {
    daemon: Arc<ServiceDaemon>,
    fullname: String,
}

impl AnnouncementHandle {
    /// Stop announcing the service. Safe to call multiple times; the
    /// `Drop` impl performs the same work if not called explicitly.
    pub fn shutdown(self) {
        // Just drop self.
        drop(self);
    }
}

impl Drop for AnnouncementHandle {
    fn drop(&mut self) {
        // best-effort: ignore errors during shutdown.
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Description of the share being announced.
#[derive(Debug, Clone)]
pub struct AnnounceInfo<'a> {
    pub blueprint_name: &'a str,
    pub size_bytes: u64,
    pub ship_type: &'a str,
    pub ksp_version: Option<&'a str>,
    pub port: u16,
}

/// Begin advertising a sender on the LAN. The returned handle keeps
/// the announcement live until it is dropped.
pub fn announce(info: AnnounceInfo<'_>) -> Result<AnnouncementHandle> {
    let daemon = ServiceDaemon::new().map_err(map_err)?;
    let host = format!("ksp-share-{}.local.", instance_label(info.blueprint_name));
    let mut props: Vec<(&str, String)> = vec![
        (TXT_NAME, info.blueprint_name.to_string()),
        (TXT_VERSION, PROTOCOL_VERSION.to_string()),
        (TXT_SIZE, info.size_bytes.to_string()),
        (TXT_SHIP, info.ship_type.to_string()),
    ];
    if let Some(v) = info.ksp_version {
        props.push((TXT_KSP_VERSION, v.to_string()));
    }

    let no_addrs: &[IpAddr] = &[];
    let svc = ServiceInfo::new(
        SERVICE_TYPE,
        info.blueprint_name,
        &host,
        no_addrs,
        info.port,
        &props[..],
    )
    .map_err(map_err)?
    .enable_addr_auto();

    let fullname = svc.get_fullname().to_string();
    daemon.register(svc).map_err(map_err)?;
    Ok(AnnouncementHandle {
        daemon: Arc::new(daemon),
        fullname,
    })
}

/// Browse the LAN for `_ksp-share._tcp.local.` services for at most
/// `timeout`, returning every resolved instance seen during the window.
pub fn browse(timeout: Duration) -> Result<Vec<AnnouncedShare>> {
    let daemon = ServiceDaemon::new().map_err(map_err)?;
    let receiver = daemon.browse(SERVICE_TYPE).map_err(map_err)?;

    let deadline = Instant::now() + timeout;
    let mut found = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(event) => {
                if let ServiceEvent::ServiceResolved(info) = event {
                    if let Some(share) = service_to_share(&info) {
                        if !found
                            .iter()
                            .any(|f: &AnnouncedShare| f.fullname == share.fullname)
                        {
                            found.push(share);
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    Ok(found)
}

/// Browse continuously and call `on_share` each time a new share is
/// resolved. Used by `ksp-share discover`. Runs until `until_returns_false`
/// returns false, or until the daemon is dropped.
pub fn watch<F>(
    poll_interval: Duration,
    mut on_share: impl FnMut(&AnnouncedShare),
    mut until_returns_false: F,
) -> Result<()>
where
    F: FnMut() -> bool,
{
    let daemon = ServiceDaemon::new().map_err(map_err)?;
    let receiver = daemon.browse(SERVICE_TYPE).map_err(map_err)?;
    let mut seen: Vec<String> = Vec::new();
    while until_returns_false() {
        match receiver.recv_timeout(poll_interval) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if let Some(share) = service_to_share(&info) {
                    if !seen.iter().any(|f| f == &share.fullname) {
                        seen.push(share.fullname.clone());
                        on_share(&share);
                    }
                }
            }
            Ok(_) => {}
            Err(_) => {
                // timed out — give the caller a chance to exit.
                thread::yield_now();
            }
        }
    }
    let _ = daemon.shutdown();
    Ok(())
}

fn service_to_share(info: &ResolvedService) -> Option<AnnouncedShare> {
    let addr = info
        .addresses
        .iter()
        .find_map(|scoped| match scoped {
            ScopedIp::V4(ipv4) => Some(IpAddr::V4(*ipv4.addr())),
            _ => None,
        })
        .or_else(|| {
            info.addresses.iter().find_map(|scoped| match scoped {
                ScopedIp::V6(ipv6) => Some(IpAddr::V6(*ipv6.addr())),
                _ => None,
            })
        })
        .map(|ip| SocketAddr::from((ip, info.port)))?;
    let blueprint = txt_str(info, TXT_NAME).map(|s| s.to_string());
    let protocol_version = txt_str(info, TXT_VERSION).and_then(|s| s.parse().ok());
    let size_bytes = txt_str(info, TXT_SIZE).and_then(|s| s.parse().ok());
    let ship_type = txt_str(info, TXT_SHIP).map(|s| s.to_string());
    let ksp_version = txt_str(info, TXT_KSP_VERSION).map(|s| s.to_string());
    Some(AnnouncedShare {
        fullname: info.fullname.clone(),
        blueprint,
        protocol_version,
        size_bytes,
        ship_type,
        ksp_version,
        addr,
    })
}

fn txt_str<'a>(info: &'a ResolvedService, key: &str) -> Option<&'a str> {
    info.txt_properties.get_property_val_str(key)
}

fn map_err(err: mdns_sd::Error) -> Error {
    Error::Protocol(format!("mDNS error: {err}"))
}

fn instance_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "blueprint".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_label_normalises_unicode_and_punctuation() {
        assert_eq!(instance_label("Mun Rocket III"), "mun-rocket-iii");
        assert_eq!(instance_label("Rocket v2.0"), "rocket-v2-0");
        assert_eq!(instance_label("..."), "blueprint");
    }
}
