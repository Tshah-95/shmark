use crate::groups::Groups;
use crate::node::Node;
use crate::settings::Settings;
use crate::shares::Shares;
use crate::{now_secs, Device, Identity};
use anyhow::Result;
use iroh_docs::AuthorId;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

/// Daemon-wide state. Cheap to clone — everything heavy is behind Arc.
#[derive(Clone)]
pub struct AppState {
    pub identity: Arc<Identity>,
    pub device: Arc<Device>,
    pub node: Node,
    pub author: AuthorId,
    pub groups: Arc<RwLock<Groups>>,
    pub shares: Arc<Shares>,
    pub settings: Arc<RwLock<Settings>>,
    pub settings_changed: Arc<Notify>,
    pub started_at: u64,
    pub shutdown: Arc<Notify>,
}

impl AppState {
    pub async fn boot(default_display_name: &str) -> Result<Self> {
        let data_dir = crate::paths::ensure_data_dir()?;
        let identity_path = crate::paths::identity_path()?;
        let device_path = crate::paths::device_path()?;
        let groups_state_path = crate::paths::groups_state_path()?;

        let identity = Identity::load_or_create(&identity_path, default_display_name)?;
        let device = Device::load_or_create(&device_path, &identity)?;

        let node = Node::boot(device.iroh_secret.clone(), &data_dir).await?;

        // The default author is created on first boot inside iroh-docs and
        // persists across restarts via Docs::persistent. We use it as this
        // device's author for every share entry we publish.
        let author = node
            .docs
            .author_default()
            .await?;

        let groups = Groups::load(&groups_state_path)?;
        let shares = Shares::new(node.clone(), author);
        let settings = Settings::load_or_default()?;

        Ok(Self {
            identity: Arc::new(identity),
            device: Arc::new(device),
            node,
            author,
            groups: Arc::new(RwLock::new(groups)),
            shares: Arc::new(shares),
            settings: Arc::new(RwLock::new(settings)),
            settings_changed: Arc::new(Notify::new()),
            started_at: now_secs(),
            shutdown: Arc::new(Notify::new()),
        })
    }

    pub fn signal_shutdown(&self) {
        self.shutdown.notify_one();
    }

    pub fn signal_settings_changed(&self) {
        self.settings_changed.notify_waiters();
    }
}
