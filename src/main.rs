use std::process::ExitCode;

use clap::Parser;
use ksp_blueprintshare::cli::{run, Cli};
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> ExitCode {
    let filter =
        EnvFilter::try_from_env("KSP_SHARE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();

    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
