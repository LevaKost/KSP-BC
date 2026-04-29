//! Command-line interface definition and dispatcher.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::Config;
use crate::craft::CraftFile;
use crate::engine::{receive_blueprint, send_blueprint, ReceiveOptions, SendOptions};
use crate::ksp::{detect_ksp_install, KspInstall, ShipType};
use crate::Result;

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
}

#[derive(Debug, Parser)]
pub struct ListArgs {
    /// Filter by ship type.
    #[arg(long = "ship", value_enum)]
    pub ship: Option<ShipKind>,
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
        Command::Config => cmd_config(),
    }
}

fn cmd_send(args: SendArgs) -> Result<()> {
    let craft_path = resolve_craft_target(&args.target, args.ship.map(Into::into))?;
    let craft = CraftFile::load(&craft_path)?;
    println!(
        "Sharing blueprint \"{}\" ({} bytes, {})",
        craft.metadata.name,
        craft.metadata.size_bytes,
        match craft.metadata.ship_type {
            ShipType::Vab => "VAB",
            ShipType::Sph => "SPH",
            ShipType::Unknown => "Ship",
        }
    );

    let opts = SendOptions {
        bind: args.bind,
        connect_to: args.to,
    };
    send_blueprint(&craft, &opts)
}

fn cmd_receive(args: ReceiveArgs) -> Result<()> {
    let install = match args.out.clone() {
        Some(_) => None,
        None => Some(detect_ksp_install()?),
    };

    let opts = ReceiveOptions {
        connect_to: args.from,
        bind: args.bind.unwrap_or_else(default_bind),
        listen: args.from.is_none(),
        output_dir: args.out,
        ksp_install: install,
        auto_accept: args.yes,
    };
    receive_blueprint(&opts)
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
