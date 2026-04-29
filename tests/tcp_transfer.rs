//! End-to-end smoke test: send a craft over TCP between two threads
//! running on different ports and verify the file lands on disk with the
//! expected SHA-256.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use ksp_blueprintshare::craft::{sha256_hex, CraftFile};
use ksp_blueprintshare::engine::{receive_blueprint, send_blueprint, ReceiveOptions, SendOptions};
use tempfile::TempDir;

fn local_addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().unwrap()
}

#[test]
fn end_to_end_tcp_transfer_round_trip() {
    let workdir = TempDir::new().unwrap();
    let craft_path = workdir.path().join("Mun Rocket III.craft");
    let body = b"ship = Mun Rocket III\nversion = 1.12.5\ntype = VAB\nPART\n{ name=core }\n";
    std::fs::write(&craft_path, body).unwrap();
    let craft = CraftFile::load(&craft_path).unwrap();
    let expected_sha = sha256_hex(body);
    assert_eq!(craft.metadata.sha256, expected_sha);

    let out_dir = workdir.path().join("received");
    std::fs::create_dir_all(&out_dir).unwrap();

    let port = 47873u16;
    let recv_opts = ReceiveOptions {
        connect_to: None,
        bind: local_addr(port),
        listen: true,
        output_dir: Some(out_dir.clone()),
        ksp_install: None,
        auto_accept: true,
    };

    let recv_thread = thread::spawn(move || receive_blueprint(&recv_opts));

    // Give the listener a moment to bind before the sender connects.
    thread::sleep(Duration::from_millis(100));

    let send_opts = SendOptions {
        bind: local_addr(0),
        connect_to: Some(local_addr(port)),
    };
    send_blueprint(&craft, &send_opts).expect("sender failed");
    recv_thread
        .join()
        .expect("recv panicked")
        .expect("recv failed");

    let landed: PathBuf = out_dir.join("Mun Rocket III.craft");
    assert!(
        landed.exists(),
        "blueprint did not land at {}",
        landed.display()
    );
    let on_disk = std::fs::read(&landed).unwrap();
    assert_eq!(sha256_hex(&on_disk), expected_sha);
}
