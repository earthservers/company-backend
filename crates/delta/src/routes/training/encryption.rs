use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use sha2::Sha256;

/// Server's RSA key pair for decrypting training data from clients (Layer 1).
pub struct TrainingKeyPair {
    private_key: RsaPrivateKey,
    public_key: RsaPublicKey,
}

impl TrainingKeyPair {
    /// Load from PEM files on disk.
    pub fn load_from_files(
        private_key_path: &str,
        public_key_path: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let private_pem = std::fs::read_to_string(private_key_path)?;
        let public_pem = std::fs::read_to_string(public_key_path)?;

        let private_key = RsaPrivateKey::from_pkcs8_pem(&private_pem)?;
        let public_key = RsaPublicKey::from_public_key_pem(&public_pem)?;

        if public_key.n().bits() < 2048 {
            return Err("Training RSA key must be at least 2048 bits".into());
        }

        Ok(Self {
            private_key,
            public_key,
        })
    }

    /// Return the public key as a PEM string (distributed to clients).
    pub fn public_key_pem(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(self.public_key.to_public_key_pem(LineEnding::LF)?)
    }

    /// Decrypt data that was encrypted with the public key (Layer 1: RSA-OAEP-SHA256).
    pub fn decrypt_from_client(
        &self,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let padding = Oaep::new::<Sha256>();
        let decrypted = self.private_key.decrypt(padding, encrypted)?;
        Ok(decrypted)
    }
}

/// AES-256-GCM encryption for database storage (Layer 2).
pub struct DatabaseEncryption {
    key: Key<Aes256Gcm>,
}

impl DatabaseEncryption {
    /// Create from a hex-encoded 256-bit master key.
    pub fn from_hex(hex_key: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let key_bytes = hex::decode(hex_key)?;
        if key_bytes.len() != 32 {
            return Err("DB master key must be 32 bytes (64 hex chars)".into());
        }

        Ok(Self {
            key: *Key::<Aes256Gcm>::from_slice(&key_bytes),
        })
    }

    /// Encrypt plaintext for database storage.
    /// Output format: 12-byte nonce || ciphertext+tag.
    pub fn encrypt_for_storage(
        &self,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let cipher = Aes256Gcm::new(&self.key);

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("AES-GCM encryption failed: {e}"))?;

        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypt data previously encrypted with `encrypt_for_storage`.
    pub fn decrypt_from_storage(
        &self,
        encrypted: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        if encrypted.len() < 12 {
            return Err("Encrypted data too short (missing nonce)".into());
        }

        let cipher = Aes256Gcm::new(&self.key);
        let nonce = Nonce::from_slice(&encrypted[..12]);

        let plaintext = cipher
            .decrypt(nonce, &encrypted[12..])
            .map_err(|e| format!("AES-GCM decryption failed: {e}"))?;

        Ok(plaintext)
    }
}
