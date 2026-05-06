use crate::identity::Identity;
use crate::now_secs;
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// The unsigned cert body. Order of fields must remain stable — the signature
/// is over the JSON serialization of this struct, and serde_json preserves
/// declaration order.
#[derive(Serialize, Deserialize, Clone)]
pub struct DeviceCert {
    pub identity_pubkey_hex: String,
    pub node_pubkey_hex: String,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SignedDeviceCert {
    pub cert: DeviceCert,
    pub signature_hex: String,
}

impl SignedDeviceCert {
    pub fn create(identity: &Identity, node_pubkey_bytes: [u8; 32]) -> Result<Self> {
        let cert = DeviceCert {
            identity_pubkey_hex: identity.pubkey_hex(),
            node_pubkey_hex: hex::encode(node_pubkey_bytes),
            created_at: now_secs(),
        };
        let payload = serde_json::to_vec(&cert)?;
        let signature = identity.sign(&payload);
        Ok(Self {
            cert,
            signature_hex: hex::encode(signature.to_bytes()),
        })
    }

    pub fn verify(&self) -> Result<()> {
        let id_bytes = hex::decode(&self.cert.identity_pubkey_hex)
            .context("decode identity_pubkey_hex")?;
        let id_bytes: [u8; 32] = id_bytes
            .as_slice()
            .try_into()
            .context("identity_pubkey must be 32 bytes")?;
        let vk = VerifyingKey::from_bytes(&id_bytes).context("invalid identity pubkey")?;

        let sig_bytes = hex::decode(&self.signature_hex).context("decode signature_hex")?;
        let sig_bytes: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .context("signature must be 64 bytes")?;
        let signature = Signature::from_bytes(&sig_bytes);

        let payload = serde_json::to_vec(&self.cert)?;
        vk.verify(&payload, &signature)
            .context("device cert signature invalid")?;
        Ok(())
    }
}

/// One device's persistent state: its iroh secret key + cert linking it to
/// the identity. The iroh secret key is the device's network identity; the
/// cert proves the device belongs to a human identity.
pub struct Device {
    pub iroh_secret: iroh::SecretKey,
    pub cert: SignedDeviceCert,
}

#[derive(Serialize, Deserialize)]
struct StoredDevice {
    /// 32-byte iroh node secret key, hex-encoded.
    iroh_secret_hex: String,
    cert: SignedDeviceCert,
}

impl Device {
    pub fn node_pubkey_bytes(&self) -> [u8; 32] {
        self.iroh_secret.public().as_bytes().to_owned()
    }

    pub fn node_pubkey_hex(&self) -> String {
        hex::encode(self.node_pubkey_bytes())
    }

    pub fn create(identity: &Identity) -> Result<Self> {
        let iroh_secret = iroh::SecretKey::generate();
        let node_pubkey_bytes = iroh_secret.public().as_bytes().to_owned();
        let cert = SignedDeviceCert::create(identity, node_pubkey_bytes)?;
        Ok(Self { iroh_secret, cert })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path)
            .with_context(|| format!("read device from {}", path.display()))?;
        let stored: StoredDevice = serde_json::from_str(&s)
            .with_context(|| format!("parse device at {}", path.display()))?;
        let secret_bytes = hex::decode(&stored.iroh_secret_hex).context("decode iroh_secret")?;
        let secret_bytes: [u8; 32] = secret_bytes
            .as_slice()
            .try_into()
            .context("iroh_secret must be 32 bytes")?;
        let iroh_secret = iroh::SecretKey::from_bytes(&secret_bytes);
        Ok(Self {
            iroh_secret,
            cert: stored.cert,
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let stored = StoredDevice {
            iroh_secret_hex: hex::encode(self.iroh_secret.to_bytes()),
            cert: self.cert.clone(),
        };
        let s = serde_json::to_string_pretty(&stored)?;
        fs::write(path, s).with_context(|| format!("write device to {}", path.display()))?;
        crate::identity::set_secret_perms(path)?;
        Ok(())
    }

    pub fn load_or_create(path: &Path, identity: &Identity) -> Result<Self> {
        if path.exists() {
            let dev = Self::load(path)?;
            dev.cert.verify()?;
            Ok(dev)
        } else {
            let dev = Self::create(identity)?;
            dev.save(path)?;
            Ok(dev)
        }
    }
}
