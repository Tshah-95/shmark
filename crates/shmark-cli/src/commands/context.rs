use crate::client;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use shmark_core::paths;

#[derive(Subcommand)]
pub enum ContextCmd {
    /// Print the markdown context blob agents prepend before deciding
    /// where to share. Includes group + contact routing notes.
    Dump,

    /// Resolve a free-form name (group or contact) → routing target.
    /// Used by agents converting "share to garrett" into the right
    /// destination.
    Resolve { query: String },
}

pub async fn run(cmd: ContextCmd) -> Result<()> {
    let socket = paths::socket_path()?;
    match cmd {
        ContextCmd::Dump => {
            let v = client::call(&socket, "context_dump").await?;
            // Print just the markdown so it's pipeable.
            if let Some(md) = v.get("markdown").and_then(|m| m.as_str()) {
                print!("{md}");
            } else {
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        }
        ContextCmd::Resolve { query } => {
            let v = client::call_with_params(
                &socket,
                "resolve_recipient",
                json!({ "query": query }),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
    }
    Ok(())
}
