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
            // Augment each group with unread_count + latest_share_at so
            // the UI can sort by recent activity and badge new shares.
            let groups = state.groups.read().await.list();
            let my_identity = state.identity.pubkey_hex();
            let mut out = Vec::with_capacity(groups.len());
            for g in groups {
                let shares = state.shares.list(&g).await.unwrap_or_default();
                let latest_share_at = shares
                    .iter()
                    .map(|s| s.created_at)
                    .max()
                    .unwrap_or(0);
                let unread_count = shares
                    .iter()
                    .filter(|s| {
                        s.author_identity != my_identity && s.created_at > g.last_seen_at
                    })
                    .count();
                out.push(json!({
                    "namespace_id": g.namespace_id,
                    "local_alias": g.local_alias,
                    "created_locally": g.created_locally,
                    "joined_at": g.joined_at,
                    "last_seen_at": g.last_seen_at,
                    "latest_share_at": latest_share_at,
                    "unread_count": unread_count,
                }));
            }
            // Sort by latest activity desc, falling back to joined_at.
            out.sort_by(|a, b| {
                let a_latest = a["latest_share_at"].as_u64().unwrap_or(0);
                let b_latest = b["latest_share_at"].as_u64().unwrap_or(0);
                let a_joined = a["joined_at"].as_u64().unwrap_or(0);
                let b_joined = b["joined_at"].as_u64().unwrap_or(0);
                b_latest
                    .cmp(&a_latest)
                    .then_with(|| b_joined.cmp(&a_joined))
            });
            Ok(serde_json::to_value(out)?)
        }

        "groups_mark_seen" => {
            #[derive(Deserialize)]
            struct P {
                name_or_id: String,
            }
            let p: P = serde_json::from_value(params)?;
            let g = state
                .groups
                .write()
                .await
                .mark_seen(&p.name_or_id, shmark_core::now_secs())?;
            Ok(serde_json::to_value(g)?)
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

            let auto_pin = state.settings.read().await.auto_pin;
            let my_identity = state.identity.pubkey_hex();

            let mut out = Vec::new();
            for g in groups {
                let shares = state.shares.list_with_status(&g).await?;
                for s in shares {
                    // If auto_pin is on and there's a non-local blob from
                    // someone other than us, kick off a background fetch.
                    // The fetch is best-effort and won't block this list.
                    if auto_pin && !s.all_local && s.record.author_identity != my_identity {
                        let shares_clone = state.shares.clone();
                        let record_clone = s.record.clone();
                        tokio::spawn(async move {
                            shares_clone.auto_pin(&record_clone).await;
                        });
                    }
                    out.push(json!({
                        "group": g.local_alias,
                        "namespace_id": g.namespace_id,
                        "share": s.record,
                        "items_status": s.items_status,
                        "all_local": s.all_local,
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

            // Ask the host (shmark-tauri or shmark-cli foreground) to
            // re-bootstrap the in-memory AppState so callers can keep
            // working without an external process restart. The new
            // identity is already on disk; reload picks it up.
            state.signal_reload();

            Ok(json!({
                "identity_pubkey": resp.identity_pubkey_hex,
                "imported_groups": imported_aliases,
                "reload_requested": true,
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

        "contacts_list" => {
            let v = state.contacts.read().await.list();
            Ok(serde_json::to_value(v)?)
        }

        "contacts_upsert" => {
            #[derive(Deserialize)]
            struct P {
                identity_pubkey: String,
                display_name: String,
            }
            let p: P = serde_json::from_value(params)?;
            let c = state
                .contacts
                .write()
                .await
                .upsert(p.identity_pubkey, p.display_name)?;
            Ok(serde_json::to_value(c)?)
        }

        "contacts_remove" => {
            #[derive(Deserialize)]
            struct P {
                name_or_pubkey: String,
            }
            let p: P = serde_json::from_value(params)?;
            let removed = state.contacts.write().await.remove(&p.name_or_pubkey)?;
            Ok(serde_json::to_value(removed)?)
        }

        "contacts_set_note" => {
            #[derive(Deserialize)]
            struct P {
                name_or_pubkey: String,
                #[serde(default)]
                note: Option<String>,
            }
            let p: P = serde_json::from_value(params)?;
            let c = state
                .contacts
                .write()
                .await
                .set_contact_note(&p.name_or_pubkey, p.note)?;
            Ok(serde_json::to_value(c)?)
        }

        "groups_set_note" => {
            #[derive(Deserialize)]
            struct P {
                group: String,
                #[serde(default)]
                note: Option<String>,
            }
            let p: P = serde_json::from_value(params)?;
            // Resolve the group alias so we store under the canonical name.
            let group = state.groups.read().await.resolve(&p.group)?;
            state
                .contacts
                .write()
                .await
                .set_group_note(&group.local_alias, p.note)?;
            Ok(json!({ "group": group.local_alias }))
        }

        "context_dump" => {
            // Returns a markdown blob assembling all routing notes the
            // agent should consider before deciding where to share. Format
            // is stable (h1: shmark context, h2 sections, h3 entries).
            let groups_snapshot = state.groups.read().await.list();
            let contacts_snapshot = state.contacts.read().await;
            let mut out = String::new();
            out.push_str("# shmark context\n\n");

            out.push_str("## Identity\n\n");
            out.push_str(&format!(
                "- display name: {}\n- identity_pubkey: {}\n\n",
                state.identity.display_name,
                state.identity.pubkey_hex()
            ));

            out.push_str("## Groups\n\n");
            if groups_snapshot.is_empty() {
                out.push_str("(no groups yet)\n\n");
            } else {
                for g in &groups_snapshot {
                    out.push_str(&format!("### {}\n\n", g.local_alias));
                    if let Some(note) = contacts_snapshot.group_note(&g.local_alias) {
                        out.push_str(note);
                        out.push_str("\n\n");
                    } else {
                        out.push_str("(no note)\n\n");
                    }
                }
            }

            let contacts = contacts_snapshot.list();
            out.push_str("## Contacts\n\n");
            if contacts.is_empty() {
                out.push_str("(no contacts yet)\n\n");
            } else {
                for c in contacts {
                    out.push_str(&format!(
                        "### {} ({})\n\n",
                        c.display_name,
                        &c.identity_pubkey[..c.identity_pubkey.len().min(12)]
                    ));
                    if let Some(note) = c.note {
                        out.push_str(&note);
                        out.push_str("\n\n");
                    } else {
                        out.push_str("(no note)\n\n");
                    }
                }
            }
            Ok(json!({ "markdown": out }))
        }

        "resolve_recipient" => {
            // The "share to <name>" agent endpoint. Given a free-form
            // string, return either:
            //   { kind: "group", group: ... }     — single match in groups
            //   { kind: "contact", contact: ... } — single match in contacts
            //   { kind: "candidates", candidates: [...] } — ambiguous
            //   { kind: "none" } — nothing matched
            //
            // For v0, contacts that match resolve to "share to a 1:1 group
            // with that contact" — but we don't have 1:1 groups yet. So
            // contact resolution is informational; the agent must still
            // pick a group to share to. We surface contacts here so the
            // agent can prompt the user with "create a 1:1 with X?".
            #[derive(Deserialize)]
            struct P {
                query: String,
            }
            let p: P = serde_json::from_value(params)?;
            let q = p.query.trim();

            let groups = state.groups.read().await;
            let group_match = groups.resolve(q).ok();
            let contacts = state.contacts.read().await;
            let contact_match = contacts.resolve(q).ok();

            match (group_match, contact_match) {
                (Some(g), None) => Ok(json!({ "kind": "group", "group": g })),
                (None, Some(c)) => Ok(json!({ "kind": "contact", "contact": c })),
                (Some(g), Some(c)) => Ok(json!({
                    "kind": "candidates",
                    "candidates": [
                        {"kind": "group", "group": g},
                        {"kind": "contact", "contact": c}
                    ]
                })),
                (None, None) => Ok(json!({ "kind": "none", "query": q })),
            }
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
