use crate::client;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use shmark_core::paths;

#[derive(Subcommand)]
pub enum DevicesCmd {
    /// Mint a one-shot pairing code that another device can use to join
    /// this identity. The code embeds this device's network address and a
    /// 5-minute token.
    Pair {
        /// Pairing code from another device. If provided, this device
        /// joins that identity instead of minting a new code.
        code: Option<String>,
        /// Display name to send back to the existing device. Defaults to
        /// the local one.
        #[arg(long)]
        display_name: Option<String>,
    },

    /// List devices known for this identity.
    List,
}

pub async fn run(cmd: DevicesCmd) -> Result<()> {
    let socket = paths::socket_path()?;
    let value = match cmd {
        DevicesCmd::Pair {
            code: Some(code),
            display_name,
        } => {
            let v = client::call_with_params(
                &socket,
                "devices_pair_join",
                json!({ "code": code, "display_name": display_name }),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
            println!();
            println!("✓ pairing complete. Restart shmark for the new identity to take effect.");
            return Ok(());
        }
        DevicesCmd::Pair { code: None, .. } => {
            client::call(&socket, "devices_pair_create").await?
        }
        DevicesCmd::List => client::call(&socket, "devices_list").await?,
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
