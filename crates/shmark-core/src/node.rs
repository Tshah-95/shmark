use crate::SHMARK_ALPN;
use anyhow::{Context, Result};
use iroh::{endpoint::presets, Endpoint, SecretKey};

pub async fn bind_endpoint(secret_key: SecretKey) -> Result<Endpoint> {
    Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![SHMARK_ALPN.to_vec()])
        .bind()
        .await
        .context("bind iroh endpoint")
}
