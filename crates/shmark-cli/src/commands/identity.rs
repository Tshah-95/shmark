use crate::client;
use anyhow::Result;
use clap::Subcommand;
use shmark_core::paths;

#[derive(Subcommand)]
pub enum IdentityCmd {
    /// Show this identity's pubkey, display name, and the device cert.
    Show,
}

pub async fn run(cmd: IdentityCmd) -> Result<()> {
    match cmd {
        IdentityCmd::Show => show().await,
    }
}

async fn show() -> Result<()> {
    let socket = paths::socket_path()?;
    let value = client::call(&socket, "identity_show").await?;
    let pretty = serde_json::to_string_pretty(&value)?;
    println!("{pretty}");
    Ok(())
}
