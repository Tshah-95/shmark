use crate::{now_secs, Device, Identity};
use anyhow::Result;
use iroh::Endpoint;
use std::sync::Arc;
use tokio::sync::Notify;

/// Daemon-wide state. Cheap to clone — everything heavy is behind Arc.
#[derive(Clone)]
pub struct AppState {
    pub identity: Arc<Identity>,
    pub device: Arc<Device>,
    pub endpoint: Endpoint,
    pub started_at: u64,
    pub shutdown: Arc<Notify>,
}

impl AppState {
    pub async fn boot(default_display_name: &str) -> Result<Self> {
        let _data_dir = crate::paths::ensure_data_dir()?;
        let identity_path = crate::paths::identity_path()?;
        let device_path = crate::paths::device_path()?;

        let identity = Identity::load_or_create(&identity_path, default_display_name)?;
        let device = Device::load_or_create(&device_path, &identity)?;

        let endpoint = crate::node::bind_endpoint(device.iroh_secret.clone()).await?;

        Ok(Self {
            identity: Arc::new(identity),
            device: Arc::new(device),
            endpoint,
            started_at: now_secs(),
            shutdown: Arc::new(Notify::new()),
        })
    }

    pub fn signal_shutdown(&self) {
        self.shutdown.notify_one();
    }
}
