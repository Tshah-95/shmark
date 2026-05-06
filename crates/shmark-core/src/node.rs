use anyhow::{Context, Result};
use iroh::{endpoint::presets, protocol::Router, Endpoint, SecretKey};
use iroh_blobs::{store::fs::FsStore, BlobsProtocol, ALPN as BLOBS_ALPN};
use iroh_docs::{protocol::Docs, ALPN as DOCS_ALPN};
use iroh_gossip::{net::Gossip, ALPN as GOSSIP_ALPN};
use std::path::Path;
use std::sync::Arc;

/// Everything the daemon needs from the iroh stack — the endpoint, the blob
/// store, the gossip layer, the docs protocol, and the router that fans
/// inbound connections out to the right protocol handler.
#[derive(Clone)]
pub struct Node {
    pub endpoint: Endpoint,
    pub blobs: Arc<FsStore>,
    pub gossip: Gossip,
    pub docs: Docs,
    pub router: Router,
}

impl Node {
    /// Build the full iroh stack with persistent on-disk state.
    pub async fn boot(secret_key: SecretKey, data_dir: &Path) -> Result<Self> {
        let blobs_dir = data_dir.join("blobs");
        let docs_dir = data_dir.join("docs");
        std::fs::create_dir_all(&blobs_dir)
            .with_context(|| format!("create blobs dir {}", blobs_dir.display()))?;
        std::fs::create_dir_all(&docs_dir)
            .with_context(|| format!("create docs dir {}", docs_dir.display()))?;

        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .bind()
            .await
            .context("bind iroh endpoint")?;

        let blobs = FsStore::load(&blobs_dir)
            .await
            .context("load iroh-blobs FsStore")?;
        let blobs = Arc::new(blobs);

        let gossip = Gossip::builder().spawn(endpoint.clone());

        let docs = Docs::persistent(docs_dir)
            .spawn(endpoint.clone(), (**blobs).clone(), gossip.clone())
            .await
            .context("spawn iroh-docs")?;

        let router = Router::builder(endpoint.clone())
            .accept(BLOBS_ALPN, BlobsProtocol::new(blobs.as_ref(), None))
            .accept(GOSSIP_ALPN, gossip.clone())
            .accept(DOCS_ALPN, docs.clone())
            .spawn();

        Ok(Self {
            endpoint,
            blobs,
            gossip,
            docs,
            router,
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.router.shutdown().await.context("router shutdown")?;
        self.endpoint.close().await;
        Ok(())
    }
}
