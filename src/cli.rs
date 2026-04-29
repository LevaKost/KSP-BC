//! Command-line interface definition and dispatcher.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::Config;
use crate::craft::CraftFile;
use crate::engine::{receive_blueprint, send_blueprint, ReceiveOptions, SendOptions};
use crate::ksp::{detect_ksp_install, KspInstall, ShipType};
use crate::transport::mdns;
use crate::{Error, Result};

/// Default TCP port advertised by `ksp-share send` when none is supplied.
pub const DEFAULT_PORT: u16 = 7878;

/// Top-level CLI definition.
#[derive(Debug, Parser)]
#[command(
    name = "ksp-share",
    version,
    about = "P2P sharing of Kerbal Space Program craft files",
    long_about = "Send and receive .craft blueprints between friends with a single command."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands exposed by the CLI.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Send a blueprint to a connecting peer.
    Send(SendArgs),
    /// Receive a blueprint from a sending peer.
    Receive(ReceiveArgs),
    /// List blueprints found in the local KSP installation.
    List(ListArgs),
    /// Discover active senders on the local network via mDNS.
    Discover(DiscoverArgs),
    /// Show detected configuration and KSP install paths.
    Config,
}

#[derive(Debug, Parser)]
pub struct SendArgs {
    /// Path to a .craft file or the blueprint name (resolved against the local KSP install).
    pub target: String,

    /// Address to bind on. Defaults to `0.0.0.0:7878`.
    #[arg(long = "bind", default_value_t = default_bind())]
    pub bind: SocketAddr,

    /// Connect to this address instead of binding (active sender mode).
    #[arg(long = "to")]
    pub to: Option<SocketAddr>,

    /// Override the ship category when resolving by name.
    #[arg(long = "ship", value_enum)]
    pub ship: Option<ShipKind>,

    /// Don't publish a `_ksp-share._tcp.local.` mDNS record.
    #[arg(long = "no-mdns")]
    pub no_mdns: bool,
}

#[derive(Debug, Parser)]
pub struct ReceiveArgs {
    /// Connect to a sender at this address.
    #[arg(long = "from")]
    pub from: Option<SocketAddr>,

    /// Bind here and wait for a sender to connect (passive receiver mode).
    #[arg(long = "bind")]
    pub bind: Option<SocketAddr>,

    /// Override the destination directory (defaults to the detected KSP Ships dir).
    #[arg(long = "out")]
    pub out: Option<PathBuf>,

    /// Auto-accept the transfer without asking.
    #[arg(long = "yes", short = 'y')]
    pub yes: bool,

    /// Skip LAN auto-discovery via mDNS.
    #[arg(long = "no-mdns")]
    pub no_mdns: bool,

    /// How long to browse for LAN announcements before giving up.
    #[arg(long = "discover-timeout", default_value_t = 4u64)]
    pub discover_timeout_secs: u64,
}

#[derive(Debug, Parser)]
pub struct ListArgs {
    /// Filter by ship type.
    #[arg(long = "ship", value_enum)]
    pub ship: Option<ShipKind>,
}

#[derive(Debug, Parser)]
pub struct DiscoverArgs {
    /// How long to browse for. `0` means run until interrupted.
    #[arg(long = "timeout", default_value_t = 5u64)]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ShipKind {
    Vab,
    Sph,
}

impl From<ShipKind> for ShipType {
    fn from(value: ShipKind) -> Self {
        match value {
            ShipKind::Vab => ShipType::Vab,
            ShipKind::Sph => ShipType::Sph,
        }
    }
}

fn default_bind() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))
}

/// Dispatch a parsed CLI invocation.
pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Send(args) => cmd_send(args),
        Command::Receive(args) => cmd_receive(args),
        Command::List(args) => cmd_list(args),
        Command::Discover(args) => cmd_discover(args),
        Command::Config => cmd_config(),
    }
}

fn cmd_send(args: SendArgs) -> Result<()> {
    let craft_path = resolve_craft_target(&args.target, args.ship.map(Into::into))?;
    let craft = CraftFile::load(&craft_path)?;
    let ship_label = ship_label(craft.metadata.ship_type);
    println!(
        "Sharing blueprint \"{}\" ({} bytes, {})",
        craft.metadata.name, craft.metadata.size_bytes, ship_label
    );

    let (opts, mdns_handle) = if let Some(addr) = args.to {
        (SendOptions::Connect(addr), None)
    } else {
        let (opts, local) = SendOptions::bind(args.bind)?;
        println!("Listening on {local} — share this address with the receiver");
        let handle = if !args.no_mdns {
            match mdns::announce(mdns::AnnounceInfo {
                blueprint_name: &craft.metadata.name,
                size_bytes: craft.metadata.size_bytes,
                ship_type: ship_label,
                ksp_version: craft.metadata.ksp_version.as_deref(),
                port: local.port(),
            }) {
                Ok(handle) => {
                    println!(
                        "Announcing on LAN as `{}` (mDNS service `{}`, port {})",
                        craft.metadata.name,
                        mdns::SERVICE_TYPE,
                        local.port()
                    );
                    Some(handle)
                }
                Err(err) => {
                    eprintln!("warning: failed to publish mDNS record: {err}");
                    None
                }
            }
        } else {
            None
        };
        (opts, handle)
    };

    let result = send_blueprint(&craft, opts);
    drop(mdns_handle);
    result
}

fn ship_label(ship: ShipType) -> &'static str {
    match ship {
        ShipType::Vab => "VAB",
        ShipType::Sph => "SPH",
        ShipType::Unknown => "Ship",
    }
}

fn cmd_receive(args: ReceiveArgs) -> Result<()> {
    let install = match args.out.clone() {
        Some(_) => None,
        None => Some(detect_ksp_install()?),
    };

    // Resolution order:
    //   1. explicit --from <addr>          → dial out
    //   2. explicit --bind <addr>          → bind & wait
    //   3. otherwise (mDNS allowed)        → browse LAN, then dial out
    //   4. fallback                        → bind on the default port
    let (connect_to, bind, listen) = if let Some(addr) = args.from {
        (Some(addr), default_bind(), false)
    } else if let Some(addr) = args.bind {
        (None, addr, true)
    } else if !args.no_mdns {
        match discover_one(args.discover_timeout_secs)? {
            Some(addr) => (Some(addr), default_bind(), false),
            None => (None, default_bind(), true),
        }
    } else {
        (None, default_bind(), true)
    };

    let opts = ReceiveOptions {
        connect_to,
        bind,
        listen,
        output_dir: args.out,
        ksp_install: install,
        auto_accept: args.yes,
    };
    receive_blueprint(&opts)
}

fn discover_one(timeout_secs: u64) -> Result<Option<SocketAddr>> {
    if timeout_secs == 0 {
        return Ok(None);
    }
    println!(
        "Browsing LAN for senders ({}s, Ctrl-C to cancel)…",
        timeout_secs
    );
    let shares = mdns::browse(Duration::from_secs(timeout_secs))?;
    match shares.len() {
        0 => {
            println!("No LAN senders found.");
            Ok(None)
        }
        1 => {
            let s = &shares[0];
            print_share(s);
            println!("→ connecting to {}", s.addr);
            Ok(Some(s.addr))
        }
        _ => {
            for (idx, share) in shares.iter().enumerate() {
                println!("  [{idx}] {} @ {}", display_name(share), share.addr);
            }
            print!("Pick a sender [0]: ");
            std::io::Write::flush(&mut std::io::stdout()).ok();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            let idx: usize = answer.trim().parse().unwrap_or(0);
            let share = shares
                .get(idx)
                .ok_or_else(|| Error::Protocol(format!("invalid selection: {idx}")))?;
            Ok(Some(share.addr))
        }
    }
}

fn display_name(share: &mdns::AnnouncedShare) -> String {
    share
        .blueprint
        .clone()
        .unwrap_or_else(|| share.fullname.clone())
}

fn print_share(share: &mdns::AnnouncedShare) {
    let name = display_name(share);
    let size = share
        .size_bytes
        .map(|s| format!("{s} bytes"))
        .unwrap_or_else(|| "?".into());
    let ship = share.ship_type.clone().unwrap_or_else(|| "?".into());
    println!(
        "Found \"{name}\" [{ship}, {size}] on {addr}",
        addr = share.addr
    );
}

fn cmd_list(args: ListArgs) -> Result<()> {
    let install = detect_ksp_install()?;
    let crafts = install.list_blueprints()?;
    let want: Option<ShipType> = args.ship.map(Into::into);
    for entry in crafts {
        if let Some(filter) = want {
            if entry.ship_type != filter {
                continue;
            }
        }
        println!(
            "[{}] {}  ({} bytes)",
            match entry.ship_type {
                ShipType::Vab => "VAB",
                ShipType::Sph => "SPH",
                ShipType::Unknown => "?",
            },
            entry.name,
            entry.size_bytes
        );
    }
    Ok(())
}

fn cmd_discover(args: DiscoverArgs) -> Result<()> {
    if args.timeout_secs == 0 {
        println!("Browsing LAN (Ctrl-C to stop)…");
        // No explicit signal handling — Ctrl-C exits the process and
        // `AnnouncementHandle::drop` plus the daemon's own shutdown
        // hook take care of releasing the multicast group.
        mdns::watch(Duration::from_secs(1), print_share, || true)?;
        return Ok(());
    }
    println!("Browsing LAN for {}s…", args.timeout_secs);
    let shares = mdns::browse(Duration::from_secs(args.timeout_secs))?;
    if shares.is_empty() {
        println!("No senders found.");
    } else {
        for share in &shares {
            print_share(share);
        }
    }
    Ok(())
}

fn cmd_config() -> Result<()> {
    let cfg = Config::load_or_default()?;
    println!("Config file:        {}", cfg.config_path().display());
    println!("Default port:       {}", cfg.port);
    match detect_ksp_install() {
        Ok(install) => print_install(&install),
        Err(err) => println!("KSP install:        not detected ({err})"),
    }
    Ok(())
}

fn print_install(install: &KspInstall) {
    println!("KSP root:           {}", install.root.display());
    println!("VAB ships dir:      {}", install.vab_dir().display());
    println!("SPH ships dir:      {}", install.sph_dir().display());
}

fn resolve_craft_target(target: &str, ship: Option<ShipType>) -> Result<PathBuf> {
    let path = Path::new(target);
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let install = detect_ksp_install()?;
    install.find_blueprint(target, ship)
}
