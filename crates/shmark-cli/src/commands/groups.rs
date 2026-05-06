use crate::client;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use shmark_core::paths;

#[derive(Subcommand)]
pub enum GroupsCmd {
    /// Create a new group with the given local alias.
    New { alias: String },

    /// List all groups this device knows about.
    List,

    /// Print a share code for a group. Defaults to write access.
    ShareCode {
        name_or_id: String,
        /// Mint a read-only share code instead of write.
        #[arg(long)]
        read_only: bool,
    },

    /// Join a group by share code. Optionally pick a local alias.
    Join {
        code: String,
        #[arg(long)]
        alias: Option<String>,
    },

    /// Rename a group's local alias on this device.
    Rename {
        name_or_id: String,
        new_alias: String,
    },

    /// Forget a group locally (does not affect peers).
    Remove { name_or_id: String },

    /// Set or clear a routing note for a group. Use --clear to remove.
    Note {
        name_or_id: String,
        /// Note body.
        #[arg(default_value = "")]
        note: String,
        #[arg(long)]
        clear: bool,
    },
}

pub async fn run(cmd: GroupsCmd) -> Result<()> {
    let socket = paths::socket_path()?;
    let value = match cmd {
        GroupsCmd::New { alias } => {
            client::call_with_params(&socket, "groups_new", json!({ "alias": alias })).await?
        }
        GroupsCmd::List => client::call(&socket, "groups_list").await?,
        GroupsCmd::ShareCode {
            name_or_id,
            read_only,
        } => {
            client::call_with_params(
                &socket,
                "groups_share_code",
                json!({ "name_or_id": name_or_id, "read_only": read_only }),
            )
            .await?
        }
        GroupsCmd::Join { code, alias } => {
            client::call_with_params(&socket, "groups_join", json!({ "code": code, "alias": alias }))
                .await?
        }
        GroupsCmd::Rename {
            name_or_id,
            new_alias,
        } => {
            client::call_with_params(
                &socket,
                "groups_rename",
                json!({ "name_or_id": name_or_id, "new_alias": new_alias }),
            )
            .await?
        }
        GroupsCmd::Remove { name_or_id } => {
            client::call_with_params(
                &socket,
                "groups_remove",
                json!({ "name_or_id": name_or_id }),
            )
            .await?
        }
        GroupsCmd::Note {
            name_or_id,
            note,
            clear,
        } => {
            let note_value: Option<String> = if clear || note.is_empty() {
                None
            } else {
                Some(note)
            };
            client::call_with_params(
                &socket,
                "groups_set_note",
                json!({ "group": name_or_id, "note": note_value }),
            )
            .await?
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
