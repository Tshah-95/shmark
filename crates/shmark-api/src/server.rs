use crate::dispatch;
use crate::protocol::{Request, Response};
use anyhow::{Context, Result};
use shmark_core::AppState;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

pub async fn serve(state: AppState, socket_path: &Path) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .with_context(|| format!("remove stale socket {}", socket_path.display()))?;
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("bind unix socket at {}", socket_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    info!(socket = %socket_path.display(), "shmark daemon listening");

    let shutdown = state.shutdown.clone();
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                info!("shutdown requested, draining");
                break;
            }
            accept = listener.accept() => {
                let (stream, _addr) = accept.context("accept unix connection")?;
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(stream, state).await {
                        warn!(error = ?e, "connection handler errored");
                    }
                });
            }
        }
    }

    let _ = std::fs::remove_file(socket_path);
    Ok(())
}

async fn handle_conn(stream: UnixStream, state: AppState) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => match dispatch(&req.method, req.params, &state).await {
                Ok(value) => Response::ok(value),
                Err(e) => Response::err("internal", flatten_chain(&e)),
            },
            Err(e) => Response::err("parse_error", e.to_string()),
        };
        let mut buf = serde_json::to_string(&resp)?;
        buf.push('\n');
        write_half.write_all(buf.as_bytes()).await?;
    }
    Ok(())
}

fn flatten_chain(e: &anyhow::Error) -> String {
    let mut msg = format!("{e}");
    for cause in e.chain().skip(1) {
        msg.push_str(&format!(": {cause}"));
    }
    msg
}
