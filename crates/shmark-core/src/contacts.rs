//! Local-only contact book + per-contact / per-group routing notes.
//!
//! "Routing notes" are free-text reminders — for the human, but mostly
//! for an agent that's deciding where to share things — about a contact
//! or a group. Persisted to data_dir/contacts.json. Never replicated to
//! peers; each device has its own view.

use crate::now_secs;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub identity_pubkey: String,
    pub display_name: String,
    pub note: Option<String>,
    pub added_at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct ContactsFile {
    #[serde(default)]
    contacts: Vec<Contact>,
    #[serde(default)]
    group_notes: BTreeMap<String, String>,
}

pub struct Contacts {
    path: PathBuf,
    by_pubkey: BTreeMap<String, Contact>,
    group_notes: BTreeMap<String, String>,
}

impl Contacts {
    pub fn load(path: &Path) -> Result<Self> {
        let (by_pubkey, group_notes) = if path.exists() {
            let s = fs::read_to_string(path)
                .with_context(|| format!("read contacts {}", path.display()))?;
            let stored: ContactsFile = serde_json::from_str(&s)
                .with_context(|| format!("parse contacts {}", path.display()))?;
            (
                stored
                    .contacts
                    .into_iter()
                    .map(|c| (c.identity_pubkey.clone(), c))
                    .collect(),
                stored.group_notes,
            )
        } else {
            (BTreeMap::new(), BTreeMap::new())
        };
        Ok(Self {
            path: path.to_path_buf(),
            by_pubkey,
            group_notes,
        })
    }

    fn save(&self) -> Result<()> {
        let stored = ContactsFile {
            contacts: self.by_pubkey.values().cloned().collect(),
            group_notes: self.group_notes.clone(),
        };
        let s = serde_json::to_string_pretty(&stored)?;
        fs::write(&self.path, s)
            .with_context(|| format!("write contacts {}", self.path.display()))?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Contact> {
        let mut v: Vec<_> = self.by_pubkey.values().cloned().collect();
        v.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        v
    }

    pub fn upsert(
        &mut self,
        identity_pubkey: String,
        display_name: String,
    ) -> Result<Contact> {
        let entry = self
            .by_pubkey
            .entry(identity_pubkey.clone())
            .or_insert(Contact {
                identity_pubkey: identity_pubkey.clone(),
                display_name: display_name.clone(),
                note: None,
                added_at: now_secs(),
            });
        entry.display_name = display_name;
        let snapshot = entry.clone();
        self.save()?;
        Ok(snapshot)
    }

    pub fn remove(&mut self, name_or_pubkey: &str) -> Result<Contact> {
        let key = self.resolve_key(name_or_pubkey)?;
        let removed = self
            .by_pubkey
            .remove(&key)
            .ok_or_else(|| anyhow!("contact disappeared mid-remove"))?;
        self.save()?;
        Ok(removed)
    }

    pub fn set_contact_note(
        &mut self,
        name_or_pubkey: &str,
        note: Option<String>,
    ) -> Result<Contact> {
        let key = self.resolve_key(name_or_pubkey)?;
        let entry = self
            .by_pubkey
            .get_mut(&key)
            .ok_or_else(|| anyhow!("contact gone mid-set"))?;
        entry.note = note;
        let snapshot = entry.clone();
        self.save()?;
        Ok(snapshot)
    }

    pub fn set_group_note(&mut self, group_alias: &str, note: Option<String>) -> Result<()> {
        match note {
            Some(n) => {
                self.group_notes.insert(group_alias.to_string(), n);
            }
            None => {
                self.group_notes.remove(group_alias);
            }
        }
        self.save()
    }

    pub fn group_note(&self, group_alias: &str) -> Option<&String> {
        self.group_notes.get(group_alias)
    }

    pub fn group_notes(&self) -> &BTreeMap<String, String> {
        &self.group_notes
    }

    pub fn resolve(&self, name_or_pubkey: &str) -> Result<Contact> {
        let key = self.resolve_key(name_or_pubkey)?;
        Ok(self.by_pubkey[&key].clone())
    }

    fn resolve_key(&self, name_or_pubkey: &str) -> Result<String> {
        if self.by_pubkey.contains_key(name_or_pubkey) {
            return Ok(name_or_pubkey.to_string());
        }
        let matches: Vec<&Contact> = self
            .by_pubkey
            .values()
            .filter(|c| c.display_name == name_or_pubkey)
            .collect();
        match matches.len() {
            0 => Err(anyhow!("no contact matches {name_or_pubkey:?}")),
            1 => Ok(matches[0].identity_pubkey.clone()),
            n => Err(anyhow!(
                "{n} contacts match {name_or_pubkey:?} — use the identity_pubkey to disambiguate"
            )),
        }
    }
}

pub fn contacts_state_path() -> Result<PathBuf> {
    Ok(crate::paths::data_dir()?.join("contacts.json"))
}
