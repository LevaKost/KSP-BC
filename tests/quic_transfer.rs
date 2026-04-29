//! End-to-end QUIC transfer test using two iroh endpoints in the same
//! process. Marked `#[ignore]` because:
//!
//! 1. It pulls in the full iroh stack and tokio runtime.
//! 2. iroh's default preset reaches out to public relay servers, which
//!    are unreliable from sandboxed CI runners. Run locally with
//!    `cargo test --features p2p --test quic_transfer -- --ignored`.

#![cfg(feature = "p2p")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use ksp_blueprintshare::craft::CraftFile;
use ksp_blueprintshare::engine::quic::{
    bind_p2p, bind_p2p_dialer, receive_blueprint_quic, send_blueprint_quic, QuicReceiveOptions,
};
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn end_to_end_quic_round_trip() {
    let workdir = TempDir::new().unwrap();
    let craft_path = workdir.path().join("Mun Rocket III.craft");
    let body = b"ship = Mun Rocket III\nversion = 1.12.5\ntype = VAB\nPART\n{ name=core }\n";
    std::fs::write(&craft_path, body).unwrap();
    let craft = CraftFile::load(&craft_path).unwrap();

    let out_dir = workdir.path().join("received");
    std::fs::create_dir_all(&out_dir).unwrap();

    let (sender_endpoint, sender_addr) = bind_p2p().await.expect("sender bind");

    // Receiver — give the sender a moment to settle then dial it.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let receiver_endpoint = bind_p2p_dialer().await.expect("receiver bind");
    let opts = QuicReceiveOptions {
        output_dir: Some(out_dir.clone()),
        ksp_install: None,
        auto_accept: true,
    };

    let sender_handle = tokio::spawn(async move {
        send_blueprint_quic(&sender_endpoint, &craft).await.unwrap();
        sender_endpoint.close().await;
    });

    receive_blueprint_quic(&receiver_endpoint, sender_addr, &opts)
        .await
        .expect("receive");
    receiver_endpoint.close().await;
    sender_handle.await.expect("sender panicked");

    let landed: PathBuf = out_dir.join("Mun Rocket III.craft");
    assert!(
        landed.exists(),
        "expected blueprint at {}",
        landed.display()
    );
    let received = std::fs::read(&landed).unwrap();
    assert_eq!(received, body);

    // Suppress unused-import warning when this test is the only consumer
    // in this file.
    let _: Option<SocketAddr> = None;
}
