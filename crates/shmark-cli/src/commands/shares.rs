use crate::client;
use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;
use shmark_core::paths;

/// `shmark share` — create a new share.
#[derive(Args)]
pub struct ShareArgs {
    /// Path to the markdown file to share.
    pub path: String,

    /// Group local alias or namespace id.
    #[arg(long, short = 't')]
    pub to: String,

    /// Display name for the share. Defaults to the file name.
    #[arg(long)]
    pub name: Option<String>,

    /// Optional description.
    #[arg(long)]
    pub description: Option<String>,
}

pub async fn run_share(args: ShareArgs) -> Result<()> {
    let socket = paths::socket_path()?;
    let value = client::call_with_params(
        &socket,
        "share_create",
        json!({
            "group": args.to,
            "path": args.path,
            "name": args.name,
            "description": args.description,
        }),
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// `shmark shares ...` — read-only commands on the share index.
#[derive(Subcommand)]
pub enum SharesCmd {
    /// List all shares (optionally filtered to a group).
    List {
        /// Restrict to a single group.
        #[arg(long)]
        group: Option<String>,
    },

    /// Download a share's items to a destination directory.
    Download {
        share_id: String,
        /// Group the share is in.
        #[arg(long)]
        group: String,
        /// Destination directory. Defaults to ./shmark-downloads/.
        #[arg(long)]
        dest: Option<String>,
    },
}

pub async fn run_shares(cmd: SharesCmd) -> Result<()> {
    let socket = paths::socket_path()?;
    let value = match cmd {
        SharesCmd::List { group } => {
            client::call_with_params(&socket, "shares_list", json!({ "group": group })).await?
        }
        SharesCmd::Download {
            share_id,
            group,
            dest,
        } => {
            client::call_with_params(
                &socket,
                "share_download",
                json!({ "share_id": share_id, "group": group, "dest": dest }),
            )
            .await?
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
