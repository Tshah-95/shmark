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

    /// Devices (multi-device pairing for one identity).
    #[command(subcommand)]
    Devices(commands::devices::DevicesCmd),

    /// Groups (DM-style containers for shares).
    #[command(subcommand)]
    Groups(commands::groups::GroupsCmd),

    /// Create a new share.
    Share(commands::shares::ShareArgs),

    /// Read-only operations on shares.
    #[command(subcommand)]
    Shares(commands::shares::SharesCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        match cli.command {
            Command::Identity(c) => commands::identity::run(c).await,
            Command::Daemon(c) => commands::daemon::run(c).await,
            Command::Devices(c) => commands::devices::run(c).await,
            Command::Groups(c) => commands::groups::run(c).await,
            Command::Share(args) => commands::shares::run_share(args).await,
            Command::Shares(c) => commands::shares::run_shares(c).await,
        }
    })
}
