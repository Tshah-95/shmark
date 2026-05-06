use crate::groups::LocalGroup;
use crate::node::Node;
use crate::now_secs;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use iroh_docs::{store::Query, AuthorId, NamespaceId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const SHARES_KEY_PREFIX: &str = "shares/";
const HTTP_USER_AGENT: &str = concat!("shmark/", env!("CARGO_PKG_VERSION"));

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

    /// Top-level entry point. Dispatches by source shape:
    ///   http(s)://...  → fetch and treat as a single-item share
    ///   /path/to/dir   → walk the directory and bundle as a folder share
    ///   /path/to/file  → single-item share
    pub async fn create(
        &self,
        group: &LocalGroup,
        source: &str,
        name: Option<String>,
        description: Option<String>,
        author_identity_hex: String,
        author_node_hex: String,
    ) -> Result<ShareRecord> {
        if source.starts_with("http://") || source.starts_with("https://") {
            return self
                .create_from_url(
                    group,
                    source,
                    name,
                    description,
                    author_identity_hex,
                    author_node_hex,
                )
                .await;
        }

        let abs = std::path::absolute(Path::new(source))
            .with_context(|| format!("absolute path for {source}"))?;
        let metadata = std::fs::metadata(&abs)
            .with_context(|| format!("stat {}", abs.display()))?;

        if metadata.is_dir() {
            return self
                .create_from_dir(
                    group,
                    &abs,
                    name,
                    description,
                    author_identity_hex,
                    author_node_hex,
                )
                .await;
        }
        if !metadata.is_file() {
            bail!("source is neither a file nor a directory: {}", abs.display());
        }

        let display_name = name.unwrap_or_else(|| {
            abs.file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
                .unwrap_or_else(|| "untitled".into())
        });
        let item = self.add_blob_from_path(&abs, None, metadata.len()).await?;
        self.publish_record(
            group,
            display_name,
            description,
            vec![item],
            author_identity_hex,
            author_node_hex,
        )
        .await
    }

    async fn create_from_url(
        &self,
        group: &LocalGroup,
        url: &str,
        name: Option<String>,
        description: Option<String>,
        author_identity_hex: String,
        author_node_hex: String,
    ) -> Result<ShareRecord> {
        let client = reqwest::Client::builder()
            .user_agent(HTTP_USER_AGENT)
            .build()
            .context("build reqwest client")?;
        let resp = client.get(url).send().await.with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            bail!("HTTP {} fetching {url}", status);
        }
        let bytes = resp.bytes().await.context("read response body")?;
        let size = bytes.len() as u64;

        // Drop content into a tempfile so iroh-blobs can stream-add from disk.
        let tmp = tempfile::NamedTempFile::new().context("create tempfile")?;
        std::fs::write(tmp.path(), &bytes).context("write tempfile")?;

        let display_name = name.unwrap_or_else(|| derive_url_filename(url));
        let item = self.add_blob_from_path(tmp.path(), None, size).await?;
        self.publish_record(
            group,
            display_name,
            description,
            vec![item],
            author_identity_hex,
            author_node_hex,
        )
        .await
    }

    async fn create_from_dir(
        &self,
        group: &LocalGroup,
        dir: &Path,
        name: Option<String>,
        description: Option<String>,
        author_identity_hex: String,
        author_node_hex: String,
    ) -> Result<ShareRecord> {
        let mut wb = ignore::WalkBuilder::new(dir);
        wb.hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .add_custom_ignore_filename(".gitignore");
        let walker = wb.build();

        let mut items = Vec::new();
        for entry in walker {
            let Ok(entry) = entry else { continue };
            let Some(ft) = entry.file_type() else { continue };
            if !ft.is_file() {
                continue;
            }
            let abs = entry.path();
            let rel = abs
                .strip_prefix(dir)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| abs.display().to_string());
            let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
            let item = self.add_blob_from_path(abs, Some(rel), size).await?;
            items.push(item);
        }
        if items.is_empty() {
            bail!("directory has no shareable files: {}", dir.display());
        }

        let display_name = name.unwrap_or_else(|| {
            dir.file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
                .unwrap_or_else(|| "folder".into())
        });
        self.publish_record(
            group,
            display_name,
            description,
            items,
            author_identity_hex,
            author_node_hex,
        )
        .await
    }

    async fn add_blob_from_path(
        &self,
        abs: &Path,
        rel_path_in_share: Option<String>,
        size_bytes: u64,
    ) -> Result<ShareItem> {
        let tag = self
            .node
            .blobs
            .blobs()
            .add_path(abs)
            .await
            .with_context(|| format!("add blob from {}", abs.display()))?;
        Ok(ShareItem {
            path: rel_path_in_share,
            blob_hash: tag.hash.to_string(),
            size_bytes,
        })
    }

    async fn publish_record(
        &self,
        group: &LocalGroup,
        name: String,
        description: Option<String>,
        items: Vec<ShareItem>,
        author_identity_hex: String,
        author_node_hex: String,
    ) -> Result<ShareRecord> {
        let record = ShareRecord {
            share_id: uuid::Uuid::new_v4().to_string(),
            name,
            description,
            items,
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
                continue;
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

    pub async fn get(&self, group: &LocalGroup, share_id: &str) -> Result<Option<ShareRecord>> {
        Ok(self
            .list(group)
            .await?
            .into_iter()
            .find(|r| r.share_id == share_id))
    }

    pub async fn read_item(
        &self,
        group: &LocalGroup,
        share_id: &str,
        item_idx: usize,
    ) -> Result<bytes::Bytes> {
        let record = self
            .get(group, share_id)
            .await?
            .ok_or_else(|| anyhow!("share not found: {share_id}"))?;
        let item = record.items.get(item_idx).ok_or_else(|| {
            anyhow!(
                "item index {item_idx} out of range (len={})",
                record.items.len()
            )
        })?;
        let hash = iroh_blobs::Hash::from_str(&item.blob_hash)
            .with_context(|| format!("parse blob hash {}", item.blob_hash))?;

        let downloader = self.node.blobs.downloader(&self.node.endpoint);
        let provider = iroh::EndpointId::from_str(&record.author_node)
            .with_context(|| format!("parse author endpoint id {}", record.author_node))?;
        downloader
            .download(hash, vec![provider])
            .await
            .with_context(|| format!("download blob {}", item.blob_hash))?;

        let bytes = self.node.blobs.blobs().get_bytes(hash).await?;
        Ok(bytes)
    }

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

        let downloader = self.node.blobs.downloader(&self.node.endpoint);
        let provider = iroh::EndpointId::from_str(&record.author_node)
            .with_context(|| format!("parse author endpoint id {}", record.author_node))?;

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
            downloader
                .download(hash, vec![provider])
                .await
                .with_context(|| format!("download blob {}", item.blob_hash))?;
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

fn derive_url_filename(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(seg) = parsed.path_segments().and_then(|s| s.last()) {
            if !seg.is_empty() {
                return seg.to_string();
            }
        }
    }
    "shared".to_string()
}
