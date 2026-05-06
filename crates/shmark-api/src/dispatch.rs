//! Pure operation dispatch — takes a method + params + AppState and returns
//! a JSON value or an error. Both transports (unix socket server, in-process
//! Tauri commands) call into this; nothing here knows or cares which one.

use anyhow::{anyhow, Result};
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use iroh_docs::DocTicket;
use serde::Deserialize;
use serde_json::{json, Value};
use shmark_core::{groups::make_local_group, AppState, LocalGroup};
use std::path::PathBuf;
use std::str::FromStr;

pub async fn dispatch(method: &str, params: Value, state: &AppState) -> Result<Value> {
    match method {
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
            let p: P = serde_json::from_value(params)?;
            let doc = state.node.docs.create().await?;
            let ns = doc.id();
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
            let p: P = serde_json::from_value(params)?;
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
            let p: P = serde_json::from_value(params)?;
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
            let p: P = serde_json::from_value(params)?;
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
            let p: P = serde_json::from_value(params)?;
            let removed = state.groups.write().await.remove(&p.name_or_id)?;
            Ok(serde_json::to_value(&removed)?)
        }

        "paths_resolve" => {
            #[derive(Deserialize)]
            struct P {
                raw: String,
            }
            let p: P = serde_json::from_value(params)?;
            let roots = shmark_core::resolve::default_roots();
            let res = shmark_core::resolve::resolve(&p.raw, &roots);
            Ok(serde_json::to_value(res)?)
        }

        "share_create" => {
            #[derive(Deserialize)]
            struct P {
                group: String,
                /// File path, directory path, or http(s) URL.
                path: String,
                name: Option<String>,
                description: Option<String>,
            }
            let p: P = serde_json::from_value(params)?;
            let group: LocalGroup = state.groups.read().await.resolve(&p.group)?;
            let record = state
                .shares
                .create(
                    &group,
                    &p.path,
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
            let p: P = serde_json::from_value(params).unwrap_or_default();
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

        "share_get_bytes" => {
            // Used by the in-app preview: download (if needed) and return the
            // file bytes as base64. Convenient for the Tauri UI without a
            // round-trip through the filesystem.
            #[derive(Deserialize)]
            struct P {
                group: String,
                share_id: String,
                /// Index of the item within the share. Defaults to 0.
                #[serde(default)]
                item: usize,
            }
            let p: P = serde_json::from_value(params)?;
            let group = state.groups.read().await.resolve(&p.group)?;
            let bytes = state.shares.read_item(&group, &p.share_id, p.item).await?;
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(json!({ "bytes_b64": b64, "len": bytes.len() }))
        }

        "share_download" => {
            #[derive(Deserialize)]
            struct P {
                group: String,
                share_id: String,
                dest: Option<String>,
            }
            let p: P = serde_json::from_value(params)?;
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
