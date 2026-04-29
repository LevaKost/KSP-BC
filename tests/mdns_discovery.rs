//! Smoke test for LAN discovery. Marked `#[ignore]` because it relies
//! on multicast UDP, which CI runners and sandboxed environments often
//! do not allow. Run locally with `cargo test --test mdns_discovery -- --ignored`.

use std::time::Duration;

use ksp_blueprintshare::transport::mdns::{announce, browse, AnnounceInfo};

#[test]
#[ignore]
fn announce_then_browse_roundtrip() {
    let _handle = announce(AnnounceInfo {
        blueprint_name: "Mun Rocket III",
        size_bytes: 1234,
        ship_type: "VAB",
        ksp_version: Some("1.12.5"),
        port: 47872,
    })
    .expect("announce");

    // Give mDNS multicast a moment to propagate.
    std::thread::sleep(Duration::from_millis(500));
    let found = browse(Duration::from_secs(3)).expect("browse");
    assert!(
        found
            .iter()
            .any(|s| s.blueprint.as_deref() == Some("Mun Rocket III")
                && s.size_bytes == Some(1234)
                && s.ship_type.as_deref() == Some("VAB")),
        "expected Mun Rocket III in {found:?}"
    );
}
