//! Multi-device pairing protocol.
//!
//! Wire flow:
//!
//!   Existing device A                 New device B
//!   -----------------                 ------------
//!   `devices_pair_create` →
//!     mint pair_token,
//!     return code (NodeAddr + token)
//!                                     (user copies code to B)
//!                                     `devices_pair_join <code>` →
//!                                     decode, dial A on shmark/pair/0,
//!                                     send PairRequest { token,
//!                                       node_pubkey, display_name }
//!   accept(), validate token,
//!   sign device cert for B's node,
//!   gather group tickets,
//!   send PairResponse { ... } →
//!                                     write identity.json, device.json
//!                                     (overwriting any local identity),
//!                                     import each group via ticket,
//!                                     `signal_shutdown()` →
//!                                     user restarts; new identity sticks.
//!
//! The token is consumed exactly once. Codes expire after 5 minutes.

use crate::device::SignedDeviceCert;
use crate::groups::Groups;
use crate::identity::Identity;
use anyhow::{anyhow, bail, Context, Result};
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::EndpointAddr;
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use iroh_docs::{protocol::Docs, NamespaceId};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub const PAIR_ALPN: &[u8] = b"shmark/pair/0";
pub const TOKEN_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_MSG_BYTES: usize = 1024 * 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairCode {
    pub addr: EndpointAddr,
    pub token: String,
}

impl PairCode {
    pub fn encode(&self) -> Result<String> {
        let bytes = postcard::to_allocvec(self)?;
        use base64::Engine;
        Ok(format!(
            "shpair-{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        ))
    }

    pub fn decode(s: &str) -> Result<Self> {
        let stripped = s
            .strip_prefix("shpair-")
            .ok_or_else(|| anyhow!("not a shmark pairing code (missing shpair- prefix)"))?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(stripped)
            .context("decode base64 pairing code")?;
        let v: Self = postcard::from_bytes(&bytes).context("postcard decode pairing code")?;
        Ok(v)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PairRequest {
    pub token: String,
    pub node_pubkey_hex: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PairResponse {
    pub identity_secret_hex: String,
    pub identity_pubkey_hex: String,
    pub identity_display_name: String,
    pub identity_created_at: u64,
    pub signed_cert: SignedDeviceCert,
    pub group_tickets: Vec<GroupTicket>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupTicket {
    pub local_alias: String,
    pub ticket: String,
}

pub struct PairingHost {
    pending: RwLock<HashMap<String, Instant>>,
}

impl PairingHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: RwLock::new(HashMap::new()),
        })
    }

    pub async fn mint_token(&self) -> String {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let hex = hex::encode(bytes);
        let mut g = self.pending.write().await;
        g.insert(hex.clone(), Instant::now());
        g.retain(|_, mint| mint.elapsed() < TOKEN_TTL);
        hex
    }

    pub async fn validate_and_consume(&self, token: &str) -> bool {
        let mut g = self.pending.write().await;
        match g.remove(token) {
            Some(mint) => mint.elapsed() < TOKEN_TTL,
            None => false,
        }
    }
}

impl Default for PairingHost {
    fn default() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Clone)]
pub struct PairProtocol {
    pub host: Arc<PairingHost>,
    pub identity: Arc<Identity>,
    pub docs: Docs,
    pub groups_state_path: PathBuf,
}

impl std::fmt::Debug for PairProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairProtocol").finish_non_exhaustive()
    }
}

impl ProtocolHandler for PairProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        if let Err(e) = self.handle(conn).await {
            tracing::warn!(error = ?e, "pairing handler errored");
        }
        Ok(())
    }
}

impl PairProtocol {
    async fn handle(&self, conn: Connection) -> Result<()> {
        let (mut send, mut recv) = conn.accept_bi().await.context("accept_bi")?;
        let bytes = recv
            .read_to_end(MAX_MSG_BYTES)
            .await
            .context("read pair request")?;
        let req: PairRequest = postcard::from_bytes(&bytes).context("decode pair request")?;

        if !self.host.validate_and_consume(&req.token).await {
            bail!("invalid or expired pairing token");
        }

        let node_bytes_vec = hex::decode(&req.node_pubkey_hex).context("decode node_pubkey hex")?;
        let node_bytes: [u8; 32] = node_bytes_vec
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("node_pubkey must be 32 bytes"))?;
        let signed_cert = SignedDeviceCert::create(&self.identity, node_bytes)?;

        let group_tickets = self.gather_group_tickets().await;

        let resp = PairResponse {
            identity_secret_hex: hex::encode(self.identity.signing_key.to_bytes()),
            identity_pubkey_hex: self.identity.pubkey_hex(),
            identity_display_name: self.identity.display_name.clone(),
            identity_created_at: self.identity.created_at,
            signed_cert,
            group_tickets,
        };
        let payload = postcard::to_allocvec(&resp).context("encode pair response")?;
        send.write_all(&payload).await.context("write pair response")?;
        send.finish().context("finish pair response stream")?;
        // Wait for the client to close the connection before we drop it.
        // Otherwise the response may still be buffered when our Connection
        // goes out of scope, and the client sees "closed by peer" before
        // reading the bytes.
        conn.closed().await;
        Ok(())
    }

    async fn gather_group_tickets(&self) -> Vec<GroupTicket> {
        let groups = match Groups::load(&self.groups_state_path) {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(error = ?e, "could not load groups for pairing");
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        for g in groups.list() {
            let Ok(ns) = NamespaceId::from_str(&g.namespace_id) else {
                continue;
            };
            let doc = match self.docs.open(ns).await {
                Ok(Some(d)) => d,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(error = ?e, "open doc during pair");
                    continue;
                }
            };
            match doc
                .share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses)
                .await
            {
                Ok(ticket) => {
                    out.push(GroupTicket {
                        local_alias: g.local_alias,
                        ticket: ticket.to_string(),
                    });
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "share doc during pair");
                }
            }
            let _ = doc.close().await;
        }
        out
    }
}

/// Client-side dial — used by `devices_pair_join`.
pub async fn pair_join(
    endpoint: &iroh::Endpoint,
    code: &str,
    node_pubkey_hex: String,
    display_name: String,
) -> Result<PairResponse> {
    let pc = PairCode::decode(code)?;
    let conn = endpoint
        .connect(pc.addr.clone(), PAIR_ALPN)
        .await
        .context("dial pair endpoint")?;
    let (mut send, mut recv) = conn.open_bi().await.context("open_bi")?;
    let req = PairRequest {
        token: pc.token,
        node_pubkey_hex,
        display_name,
    };
    let payload = postcard::to_allocvec(&req).context("encode pair request")?;
    send.write_all(&payload).await.context("write pair request")?;
    send.finish().context("finish pair request stream")?;
    let resp_bytes = recv
        .read_to_end(MAX_MSG_BYTES)
        .await
        .context("read pair response")?;
    let resp: PairResponse =
        postcard::from_bytes(&resp_bytes).context("decode pair response")?;
    resp.signed_cert.verify().context("verify signed cert")?;
    Ok(resp)
}
