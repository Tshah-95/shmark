mod client;
mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "shmark", about = "shmark — peer-to-peer markdown sharing", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Identity (the human, not a single device).
    #[command(subcommand)]
    Identity(commands::identity::IdentityCmd),

    /// Daemon lifecycle.
    #[command(subcommand)]
    Daemon(commands::daemon::DaemonCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        match cli.command {
            Command::Identity(c) => commands::identity::run(c).await,
            Command::Daemon(c) => commands::daemon::run(c).await,
        }
    })
}
