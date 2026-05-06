use crate::now_secs;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Per-group local state. The doc itself is stored by iroh-docs; this is the
/// stuff iroh-docs doesn't track for us — primarily, what *we* call this
/// group on this device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalGroup {
    /// Hex-encoded NamespaceId from iroh-docs.
    pub namespace_id: String,
    /// Local-only nickname. Friends may have a different alias for the same group.
    pub local_alias: String,
    /// True iff this device created the group (vs. joined someone else's).
    pub created_locally: bool,
    pub joined_at: u64,
}

/// In-memory index of LocalGroup, persisted to a single JSON file.
pub struct Groups {
    state_path: PathBuf,
    by_namespace: HashMap<String, LocalGroup>,
}

#[derive(Serialize, Deserialize, Default)]
struct StoredGroups {
    groups: Vec<LocalGroup>,
}

impl Groups {
    pub fn load(path: &Path) -> Result<Self> {
        let by_namespace = if path.exists() {
            let s = fs::read_to_string(path)
                .with_context(|| format!("read groups state {}", path.display()))?;
            let stored: StoredGroups = serde_json::from_str(&s)
                .with_context(|| format!("parse groups state {}", path.display()))?;
            stored
                .groups
                .into_iter()
                .map(|g| (g.namespace_id.clone(), g))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(Self {
            state_path: path.to_path_buf(),
            by_namespace,
        })
    }

    fn save(&self) -> Result<()> {
        let stored = StoredGroups {
            groups: self.by_namespace.values().cloned().collect(),
        };
        let s = serde_json::to_string_pretty(&stored)?;
        fs::write(&self.state_path, s)
            .with_context(|| format!("write groups state {}", self.state_path.display()))?;
        Ok(())
    }

    pub fn list(&self) -> Vec<LocalGroup> {
        let mut v: Vec<_> = self.by_namespace.values().cloned().collect();
        v.sort_by_key(|g| g.joined_at);
        v
    }

    pub fn upsert(&mut self, group: LocalGroup) -> Result<()> {
        self.by_namespace.insert(group.namespace_id.clone(), group);
        self.save()
    }

    pub fn rename(&mut self, name_or_id: &str, new_alias: &str) -> Result<LocalGroup> {
        let key = self.resolve_key(name_or_id)?;
        let entry = self
            .by_namespace
            .get_mut(&key)
            .ok_or_else(|| anyhow!("group disappeared mid-rename"))?;
        entry.local_alias = new_alias.to_string();
        let snapshot = entry.clone();
        self.save()?;
        Ok(snapshot)
    }

    pub fn remove(&mut self, name_or_id: &str) -> Result<LocalGroup> {
        let key = self.resolve_key(name_or_id)?;
        let removed = self
            .by_namespace
            .remove(&key)
            .ok_or_else(|| anyhow!("group disappeared mid-remove"))?;
        self.save()?;
        Ok(removed)
    }

    pub fn resolve(&self, name_or_id: &str) -> Result<LocalGroup> {
        let key = self.resolve_key(name_or_id)?;
        Ok(self.by_namespace[&key].clone())
    }

    fn resolve_key(&self, name_or_id: &str) -> Result<String> {
        if self.by_namespace.contains_key(name_or_id) {
            return Ok(name_or_id.to_string());
        }
        let matches: Vec<&LocalGroup> = self
            .by_namespace
            .values()
            .filter(|g| g.local_alias == name_or_id)
            .collect();
        match matches.len() {
            0 => Err(anyhow!("no group matches {name_or_id:?}")),
            1 => Ok(matches[0].namespace_id.clone()),
            n => Err(anyhow!(
                "{n} groups match {name_or_id:?} — use the namespace id to disambiguate"
            )),
        }
    }
}

pub fn make_local_group(namespace_id: String, alias: String, created_locally: bool) -> LocalGroup {
    LocalGroup {
        namespace_id,
        local_alias: alias,
        created_locally,
        joined_at: now_secs(),
    }
}
