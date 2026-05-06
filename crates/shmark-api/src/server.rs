use crate::protocol::{Request, Response};
use anyhow::{anyhow, Context, Result};
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use iroh_docs::DocTicket;
use serde::Deserialize;
use serde_json::json;
use shmark_core::{groups::make_local_group, AppState, LocalGroup};
use std::path::{Path, PathBuf};
use std::str::FromStr;
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
            Ok(req) => dispatch(req, &state).await,
            Err(e) => Response::err("parse_error", e.to_string()),
        };
        let mut buf = serde_json::to_string(&resp)?;
        buf.push('\n');
        write_half.write_all(buf.as_bytes()).await?;
    }
    Ok(())
}

async fn dispatch(req: Request, state: &AppState) -> Response {
    match handle(req, state).await {
        Ok(value) => Response::ok(value),
        Err(e) => {
            // Walk anyhow's context chain to surface useful messages.
            let mut msg = format!("{e}");
            for cause in e.chain().skip(1) {
                msg.push_str(&format!(": {cause}"));
            }
            Response::err("internal", msg)
        }
    }
}

async fn handle(req: Request, state: &AppState) -> Result<serde_json::Value> {
    match req.method.as_str() {
        "identity_show" => Ok(json!({
            "identity_pubkey": state.identity.pubkey_hex(),
            "display_name": state.identity.display_name,
            "created_at": state.identity.created_at,
            "device": {
                "node_pubkey": state.device.node_pubkey_hex(),
                "endpoint_id": state.node.endpoint.id().to_string(),
                "cert_created_at": state.device.cert.cert.created_at,
            },
        })),

        "daemon_status" => Ok(json!({
            "status": "running",
            "pid": std::process::id(),
            "started_at": state.started_at,
            "uptime_secs": shmark_core::now_secs().saturating_sub(state.started_at),
        })),

        "daemon_stop" => {
            state.signal_shutdown();
            Ok(json!({ "stopping": true }))
        }

        "groups_new" => {
            #[derive(Deserialize)]
            struct P {
                alias: String,
            }
            let p: P = serde_json::from_value(req.params)?;
            let doc = state.node.docs.create().await?;
            let ns = doc.id();
            // Put the doc into active sync mode so peers who join later can
            // gossip writes back to us.
            doc.start_sync(vec![]).await.ok();
            doc.close().await.ok();
            let local = make_local_group(ns.to_string(), p.alias, true);
            state.groups.write().await.upsert(local.clone())?;
            Ok(serde_json::to_value(&local)?)
        }

        "groups_list" => {
            let v = state.groups.read().await.list();
            Ok(serde_json::to_value(v)?)
        }

        "groups_share_code" => {
            #[derive(Deserialize)]
            struct P {
                name_or_id: String,
                #[serde(default)]
                read_only: bool,
            }
            let p: P = serde_json::from_value(req.params)?;
            let group = state.groups.read().await.resolve(&p.name_or_id)?;
            let ns: iroh_docs::NamespaceId = iroh_docs::NamespaceId::from_str(&group.namespace_id)
                .map_err(|e| anyhow!("parse namespace id: {e}"))?;
            let doc = state
                .node
                .docs
                .open(ns)
                .await?
                .ok_or_else(|| anyhow!("group doc not found"))?;
            let mode = if p.read_only {
                ShareMode::Read
            } else {
                ShareMode::Write
            };
            let ticket: DocTicket = doc.share(mode, AddrInfoOptions::RelayAndAddresses).await?;
            doc.close().await.ok();
            Ok(json!({
                "group": group,
                "code": ticket.to_string(),
                "mode": if p.read_only { "read" } else { "write" },
            }))
        }

        "groups_join" => {
            #[derive(Deserialize)]
            struct P {
                code: String,
                alias: Option<String>,
            }
            let p: P = serde_json::from_value(req.params)?;
            let ticket = DocTicket::from_str(p.code.trim())
                .map_err(|e| anyhow!("invalid share code: {e}"))?;
            let doc = state.node.docs.import(ticket).await?;
            let ns_id = doc.id().to_string();
            doc.close().await.ok();
            let alias = p.alias.unwrap_or_else(|| short_alias(&ns_id));
            let local = make_local_group(ns_id, alias, false);
            state.groups.write().await.upsert(local.clone())?;
            Ok(serde_json::to_value(&local)?)
        }

        "groups_rename" => {
            #[derive(Deserialize)]
            struct P {
                name_or_id: String,
                new_alias: String,
            }
            let p: P = serde_json::from_value(req.params)?;
            let updated = state
                .groups
                .write()
                .await
                .rename(&p.name_or_id, &p.new_alias)?;
            Ok(serde_json::to_value(&updated)?)
        }

        "groups_remove" => {
            #[derive(Deserialize)]
            struct P {
                name_or_id: String,
            }
            let p: P = serde_json::from_value(req.params)?;
            let removed = state.groups.write().await.remove(&p.name_or_id)?;
            // Don't drop the iroh-docs doc — keeps history if user re-adds.
            Ok(serde_json::to_value(&removed)?)
        }

        "share_create" => {
            #[derive(Deserialize)]
            struct P {
                group: String,
                path: String,
                name: Option<String>,
                description: Option<String>,
            }
            let p: P = serde_json::from_value(req.params)?;
            let group: LocalGroup = state.groups.read().await.resolve(&p.group)?;
            let record = state
                .shares
                .create(
                    &group,
                    Path::new(&p.path),
                    p.name,
                    p.description,
                    state.identity.pubkey_hex(),
                    state.device.node_pubkey_hex(),
                )
                .await?;
            Ok(serde_json::to_value(&record)?)
        }

        "shares_list" => {
            #[derive(Deserialize, Default)]
            struct P {
                #[serde(default)]
                group: Option<String>,
            }
            let p: P = serde_json::from_value(req.params).unwrap_or_default();
            let groups: Vec<LocalGroup> = if let Some(name) = p.group {
                vec![state.groups.read().await.resolve(&name)?]
            } else {
                state.groups.read().await.list()
            };
            let mut out = Vec::new();
            for g in groups {
                let shares = state.shares.list(&g).await?;
                for s in shares {
                    out.push(json!({
                        "group": g.local_alias,
                        "namespace_id": g.namespace_id,
                        "share": s,
                    }));
                }
            }
            Ok(serde_json::to_value(out)?)
        }

        "share_download" => {
            #[derive(Deserialize)]
            struct P {
                group: String,
                share_id: String,
                dest: Option<String>,
            }
            let p: P = serde_json::from_value(req.params)?;
            let group = state.groups.read().await.resolve(&p.group)?;
            let dest = match p.dest {
                Some(d) => PathBuf::from(d),
                None => std::env::current_dir()?.join("shmark-downloads"),
            };
            let written = state.shares.download(&group, &p.share_id, &dest).await?;
            Ok(json!({ "dest": written.display().to_string() }))
        }

        other => Err(anyhow!("unknown method: {other}")),
    }
}

fn short_alias(ns_id: &str) -> String {
    format!("group-{}", &ns_id[..ns_id.len().min(8)])
}
