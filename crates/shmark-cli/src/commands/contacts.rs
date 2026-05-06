use crate::client;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use shmark_core::paths;

#[derive(Subcommand)]
pub enum ContactsCmd {
    /// List local contacts.
    List,

    /// Add or update a contact.
    Add {
        /// Identity pubkey (32-byte hex from devices_list / share metadata).
        identity_pubkey: String,
        /// Display name to remember this person by locally.
        #[arg(long)]
        name: String,
    },

    /// Remove a contact.
    Remove { name_or_pubkey: String },

    /// Set or clear a routing note for a contact. Use --clear to remove.
    Note {
        name_or_pubkey: String,
        /// Note body. Omit (with --clear) to clear.
        #[arg(default_value = "")]
        note: String,
        #[arg(long)]
        clear: bool,
    },
}

pub async fn run(cmd: ContactsCmd) -> Result<()> {
    let socket = paths::socket_path()?;
    let value = match cmd {
        ContactsCmd::List => client::call(&socket, "contacts_list").await?,
        ContactsCmd::Add {
            identity_pubkey,
            name,
        } => {
            client::call_with_params(
                &socket,
                "contacts_upsert",
                json!({ "identity_pubkey": identity_pubkey, "display_name": name }),
            )
            .await?
        }
        ContactsCmd::Remove { name_or_pubkey } => {
            client::call_with_params(
                &socket,
                "contacts_remove",
                json!({ "name_or_pubkey": name_or_pubkey }),
            )
            .await?
        }
        ContactsCmd::Note {
            name_or_pubkey,
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
                "contacts_set_note",
                json!({ "name_or_pubkey": name_or_pubkey, "note": note_value }),
            )
            .await?
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
