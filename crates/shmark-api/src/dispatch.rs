//! Pure operation dispatch — takes a method + params + AppState and returns
//! a JSON value or an error. Both transports (unix socket server, in-process
//! Tauri commands) call into this; nothing here knows or cares which one.

use anyhow::{anyhow, Result};
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use iroh_docs::DocTicket;
use serde::Deserialize;
use serde_json::{json, Value};
use shmark_core::{
    dev::DevRequest,
    groups::make_local_group,
    pairing::{pair_join, PairCode},
    AppState, LocalGroup,
};
use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::oneshot;

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
            let roots = state.settings.read().await.effective_search_roots();
            let res = shmark_core::resolve::resolve(&p.raw, &roots);
            Ok(serde_json::to_value(res)?)
        }

        "settings_get" => {
            let s = state.settings.read().await.clone();
            let effective_roots: Vec<String> = s
                .effective_search_roots()
                .into_iter()
                .map(|p| p.display().to_string())
                .collect();
            Ok(json!({
                "settings": s,
                "effective_search_roots": effective_roots,
                "default_roots": shmark_core::resolve::default_roots()
                    .into_iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>(),
            }))
        }

        "settings_set" => {
            #[derive(Deserialize)]
            struct P {
                hotkey: Option<String>,
                search_roots: Option<Vec<String>>,
                auto_pin: Option<bool>,
            }
            let p: P = serde_json::from_value(params)?;
            let mut s = state.settings.write().await;
            if let Some(h) = p.hotkey {
                s.hotkey = h;
            }
            if let Some(r) = p.search_roots {
                s.search_roots = r;
            }
            if let Some(a) = p.auto_pin {
                s.auto_pin = a;
            }
            s.save()?;
            let snapshot = s.clone();
            drop(s);
            state.signal_settings_changed();
            Ok(serde_json::to_value(snapshot)?)
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

        "devices_pair_create" => {
            // Wait for the endpoint to be reachable through a relay before
            // capturing its address. Without `online()`, addr() may return
            // an EndpointAddr with no relay URL or direct addresses,
            // making the resulting code unusable.
            state
                .node
                .endpoint
                .online()
                .await;
            let token = state.pairing.mint_token().await;
            let addr = state.node.endpoint.addr();
            let pc = PairCode { addr, token };
            let code = pc.encode()?;
            Ok(json!({
                "code": code,
                "expires_in_secs": shmark_core::pairing::TOKEN_TTL.as_secs(),
            }))
        }

        "devices_pair_join" => {
            // Dial the existing device, receive its identity payload, persist
            // locally, import groups, then signal shutdown so the daemon
            // re-boots with the new identity in effect.
            #[derive(Deserialize)]
            struct P {
                code: String,
                #[serde(default)]
                display_name: Option<String>,
            }
            let p: P = serde_json::from_value(params)?;

            let display_name = p
                .display_name
                .unwrap_or_else(|| state.identity.display_name.clone());
            let resp = pair_join(
                &state.node.endpoint,
                &p.code,
                state.device.node_pubkey_hex(),
                display_name,
            )
            .await?;

            // Persist new identity over our existing one.
            let new_identity = shmark_core::Identity::from_received_secret(
                &resp.identity_secret_hex,
                resp.identity_display_name.clone(),
                resp.identity_created_at,
            )?;
            new_identity.save(&shmark_core::paths::identity_path()?)?;

            // Build new device record (existing iroh_secret + new cert) and
            // persist.
            let mut device_clone =
                shmark_core::Device::load(&shmark_core::paths::device_path()?)?;
            device_clone.replace_cert(resp.signed_cert.clone())?;
            device_clone.save(&shmark_core::paths::device_path()?)?;

            // Import each group via its ticket.
            let mut imported_aliases = Vec::new();
            for gt in &resp.group_tickets {
                let ticket = iroh_docs::DocTicket::from_str(gt.ticket.trim())
                    .map_err(|e| anyhow!("invalid ticket for {}: {e}", gt.local_alias))?;
                let doc = state.node.docs.import(ticket).await?;
                let ns_id = doc.id().to_string();
                let _ = doc.close().await;
                let local = make_local_group(ns_id, gt.local_alias.clone(), false);
                state.groups.write().await.upsert(local)?;
                imported_aliases.push(gt.local_alias.clone());
            }

            // Tell the daemon to shut down so the in-process state (which
            // still holds the old identity in Arc<Identity>) is replaced on
            // next launch.
            state.signal_shutdown();

            Ok(json!({
                "identity_pubkey": resp.identity_pubkey_hex,
                "imported_groups": imported_aliases,
                "restart_required": true,
            }))
        }

        "devices_list" => {
            // v0: just this device. v1 extends with all paired devices once
            // we have a self-doc replicating the cert chain across devices.
            Ok(json!([{
                "node_pubkey": state.device.node_pubkey_hex(),
                "identity_pubkey": state.identity.pubkey_hex(),
                "cert_created_at": state.device.cert.cert.created_at,
                "is_this_device": true,
            }]))
        }

        // Test-driver bridge — only available when shmark-tauri has
        // installed a DevSender. The standalone CLI daemon returns an error.
        "dev_emit" => {
            #[derive(Deserialize)]
            struct P {
                event: String,
                #[serde(default)]
                payload: Value,
            }
            let p: P = serde_json::from_value(params)?;
            let tx = state
                .dev_tx
                .as_ref()
                .ok_or_else(|| anyhow!("dev_* methods require shmark-desktop"))?;
            let (reply, rx) = oneshot::channel();
            tx.send(DevRequest::Emit {
                event: p.event,
                payload: p.payload,
                reply,
            })
            .map_err(|_| anyhow!("dev consumer disconnected"))?;
            rx.await
                .map_err(|_| anyhow!("dev consumer dropped reply"))?
                .map_err(|e| anyhow!("dev_emit: {e}"))?;
            Ok(json!({ "ok": true }))
        }

        "dev_window_state" => {
            let tx = state
                .dev_tx
                .as_ref()
                .ok_or_else(|| anyhow!("dev_* methods require shmark-desktop"))?;
            let (reply, rx) = oneshot::channel();
            tx.send(DevRequest::WindowState { reply })
                .map_err(|_| anyhow!("dev consumer disconnected"))?;
            let v = rx
                .await
                .map_err(|_| anyhow!("dev consumer dropped reply"))?
                .map_err(|e| anyhow!("dev_window_state: {e}"))?;
            Ok(v)
        }

        "dev_run" => {
            #[derive(Deserialize)]
            struct P {
                js: String,
            }
            let p: P = serde_json::from_value(params)?;
            let tx = state
                .dev_tx
                .as_ref()
                .ok_or_else(|| anyhow!("dev_* methods require shmark-desktop"))?;
            let (reply, rx) = oneshot::channel();
            tx.send(DevRequest::RunJs {
                js: p.js,
                reply,
            })
            .map_err(|_| anyhow!("dev consumer disconnected"))?;
            rx.await
                .map_err(|_| anyhow!("dev consumer dropped reply"))?
                .map_err(|e| anyhow!("dev_run: {e}"))?;
            Ok(json!({ "ok": true }))
        }

        "dev_run_get" => {
            #[derive(Deserialize)]
            struct P {
                js: String,
            }
            let p: P = serde_json::from_value(params)?;
            let tx = state
                .dev_tx
                .as_ref()
                .ok_or_else(|| anyhow!("dev_* methods require shmark-desktop"))?;
            let (reply, rx) = oneshot::channel();
            tx.send(DevRequest::RunJsGet {
                js: p.js,
                reply,
            })
            .map_err(|_| anyhow!("dev consumer disconnected"))?;
            let s = rx
                .await
                .map_err(|_| anyhow!("dev consumer dropped reply"))?
                .map_err(|e| anyhow!("dev_run_get: {e}"))?;
            // The webview returns a JSON string. Try to parse it; if it's
            // not valid JSON, return the raw string.
            let value = serde_json::from_str::<Value>(&s).unwrap_or(Value::String(s));
            Ok(json!({ "value": value }))
        }

        other => Err(anyhow!("unknown method: {other}")),
    }
}

fn short_alias(ns_id: &str) -> String {
    format!("group-{}", &ns_id[..ns_id.len().min(8)])
}
