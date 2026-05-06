use crate::now_secs;
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A human's stable identity. The pubkey is the identity_id and never changes.
/// Devices are linked to this identity via signed device certs.
#[derive(Clone)]
pub struct Identity {
    pub signing_key: SigningKey,
    pub display_name: String,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    /// 32-byte Ed25519 secret, hex-encoded.
    secret_key_hex: String,
    display_name: String,
    created_at: u64,
}

impl Identity {
    pub fn pubkey(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn pubkey_bytes(&self) -> [u8; 32] {
        self.pubkey().to_bytes()
    }

    pub fn pubkey_hex(&self) -> String {
        hex::encode(self.pubkey_bytes())
    }

    pub fn create(display_name: &str) -> Self {
        let mut csprng = rand::thread_rng();
        let signing_key = SigningKey::generate(&mut csprng);
        Self {
            signing_key,
            display_name: display_name.to_string(),
            created_at: now_secs(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path)
            .with_context(|| format!("read identity from {}", path.display()))?;
        let stored: StoredIdentity = serde_json::from_str(&s)
            .with_context(|| format!("parse identity at {}", path.display()))?;
        let bytes = hex::decode(&stored.secret_key_hex).context("decode identity secret_key")?;
        let bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .context("identity secret_key must be 32 bytes")?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(&bytes),
            display_name: stored.display_name,
            created_at: stored.created_at,
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let stored = StoredIdentity {
            secret_key_hex: hex::encode(self.signing_key.to_bytes()),
            display_name: self.display_name.clone(),
            created_at: self.created_at,
        };
        let s = serde_json::to_string_pretty(&stored)?;
        fs::write(path, s).with_context(|| format!("write identity to {}", path.display()))?;
        set_secret_perms(path)?;
        Ok(())
    }

    pub fn load_or_create(path: &Path, default_display_name: &str) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let id = Self::create(default_display_name);
            id.save(path)?;
            Ok(id)
        }
    }

    pub fn sign(&self, msg: &[u8]) -> Signature {
        self.signing_key.sign(msg)
    }

    /// Reconstruct an Identity from a received secret + metadata. Used when
    /// completing a multi-device pairing — Device B receives Device A's
    /// identity over the wire and persists it locally.
    pub fn from_received_secret(
        secret_hex: &str,
        display_name: String,
        created_at: u64,
    ) -> Result<Self> {
        let bytes = hex::decode(secret_hex).context("decode received identity secret")?;
        let bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .context("identity secret must be 32 bytes")?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(&bytes),
            display_name,
            created_at,
        })
    }

    pub fn verify(pubkey_bytes: &[u8; 32], msg: &[u8], signature: &Signature) -> Result<()> {
        let vk = VerifyingKey::from_bytes(pubkey_bytes).context("invalid identity pubkey")?;
        vk.verify(msg, signature).context("identity signature invalid")?;
        Ok(())
    }
}

#[cfg(unix)]
pub(crate) fn set_secret_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_secret_perms(_path: &Path) -> Result<()> {
    Ok(())
}
