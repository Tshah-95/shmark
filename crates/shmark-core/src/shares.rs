use crate::groups::LocalGroup;
use crate::node::Node;
use crate::now_secs;
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use iroh_docs::{store::Query, AuthorId, NamespaceId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const SHARES_KEY_PREFIX: &str = "shares/";

/// One file inside a share. Single-file shares have one item with `path: None`.
/// Folder shares have many items with relative paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareItem {
    pub path: Option<String>,
    pub blob_hash: String,
    pub size_bytes: u64,
}

/// The full record stored as a doc value at key "shares/<share_id>".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRecord {
    pub share_id: String,
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<ShareItem>,
    /// Identity pubkey hex of the human who authored this.
    pub author_identity: String,
    /// Iroh node pubkey hex of the device that authored this.
    pub author_node: String,
    pub created_at: u64,
}

pub struct Shares {
    node: Node,
    author: AuthorId,
}

impl Shares {
    pub fn new(node: Node, author: AuthorId) -> Self {
        Self { node, author }
    }

    /// Add `path` (a single file for now — folders TBD) as a blob and append a
    /// share entry to the group's doc.
    pub async fn create(
        &self,
        group: &LocalGroup,
        path: &Path,
        name: Option<String>,
        description: Option<String>,
        author_identity_hex: String,
        author_node_hex: String,
    ) -> Result<ShareRecord> {
        let abs = std::path::absolute(path)
            .with_context(|| format!("absolute path for {}", path.display()))?;
        let metadata = std::fs::metadata(&abs)
            .with_context(|| format!("stat {}", abs.display()))?;
        if !metadata.is_file() {
            return Err(anyhow!(
                "share <path> currently only supports a single file (folder shares TBD): {}",
                abs.display()
            ));
        }

        let tag = self
            .node
            .blobs
            .blobs()
            .add_path(&abs)
            .await
            .with_context(|| format!("add blob from {}", abs.display()))?;

        let item = ShareItem {
            path: None,
            blob_hash: tag.hash.to_string(),
            size_bytes: metadata.len(),
        };

        let display_name = name.unwrap_or_else(|| {
            abs.file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
                .unwrap_or_else(|| "untitled".to_string())
        });

        let record = ShareRecord {
            share_id: uuid::Uuid::new_v4().to_string(),
            name: display_name,
            description,
            items: vec![item],
            author_identity: author_identity_hex,
            author_node: author_node_hex,
            created_at: now_secs(),
        };

        let ns = parse_namespace_id(&group.namespace_id)?;
        let doc = self
            .node
            .docs
            .open(ns)
            .await?
            .ok_or_else(|| anyhow!("group doc not found locally: {}", group.namespace_id))?;

        let key = format!("{SHARES_KEY_PREFIX}{}", record.share_id).into_bytes();
        let value = serde_json::to_vec(&record)?;
        doc.set_bytes(self.author, key, value).await?;
        doc.close().await.ok();
        Ok(record)
    }

    pub async fn list(&self, group: &LocalGroup) -> Result<Vec<ShareRecord>> {
        let ns = parse_namespace_id(&group.namespace_id)?;
        let doc = self
            .node
            .docs
            .open(ns)
            .await?
            .ok_or_else(|| anyhow!("group doc not found locally: {}", group.namespace_id))?;

        let stream = doc.get_many(Query::key_prefix(SHARES_KEY_PREFIX)).await?;
        let mut stream = Box::pin(stream);
        let mut out = Vec::new();
        while let Some(entry) = stream.next().await {
            let entry = entry?;
            let bytes = self
                .node
                .blobs
                .blobs()
                .get_bytes(entry.content_hash())
                .await?;
            if bytes.is_empty() {
                continue; // tombstone / deletion marker
            }
            match serde_json::from_slice::<ShareRecord>(&bytes) {
                Ok(record) => out.push(record),
                Err(e) => {
                    tracing::warn!(error = ?e, "skipping malformed share entry");
                }
            }
        }
        doc.close().await.ok();
        out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        Ok(out)
    }

    /// Resolve a share by id within a group.
    pub async fn get(&self, group: &LocalGroup, share_id: &str) -> Result<Option<ShareRecord>> {
        Ok(self
            .list(group)
            .await?
            .into_iter()
            .find(|r| r.share_id == share_id))
    }

    /// Download all items of a share to a destination directory (single-file
    /// shares are written as `<dest>/<name>`; folder shares preserve relative
    /// paths). Returns the destination root.
    pub async fn download(
        &self,
        group: &LocalGroup,
        share_id: &str,
        dest_root: &Path,
    ) -> Result<PathBuf> {
        let record = self
            .get(group, share_id)
            .await?
            .ok_or_else(|| anyhow!("share not found: {share_id}"))?;

        std::fs::create_dir_all(dest_root)
            .with_context(|| format!("create dest dir {}", dest_root.display()))?;

        for item in &record.items {
            let hash = iroh_blobs::Hash::from_str(&item.blob_hash)
                .with_context(|| format!("parse blob hash {}", item.blob_hash))?;
            let target = match &item.path {
                Some(rel) => dest_root.join(rel),
                None => dest_root.join(&record.name),
            };
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            self.node
                .blobs
                .blobs()
                .export(hash, &target)
                .await
                .with_context(|| format!("export blob to {}", target.display()))?;
        }
        Ok(dest_root.to_path_buf())
    }
}

fn parse_namespace_id(s: &str) -> Result<NamespaceId> {
    NamespaceId::from_str(s).with_context(|| format!("parse NamespaceId from {s:?}"))
}
