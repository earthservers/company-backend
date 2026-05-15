use company_result::{create_error, Result};
use rsa::pkcs8::DecodePublicKey;
use rsa::traits::PublicKeyParts;
use rsa::{Oaep, RsaPublicKey};
use sha2::Sha256;

/// Validate PEM public key format and minimum key size (2048 bits)
pub fn validate_public_key(pem: &str) -> Result<RsaPublicKey> {
    let key = RsaPublicKey::from_public_key_pem(pem).map_err(|e| {
        create_error!(InvalidPublicKey {
            message: format!("Invalid PEM public key: {e}")
        })
    })?;

    if key.n().bits() < 2048 {
        return Err(create_error!(InvalidPublicKey {
            message: "RSA key must be at least 2048 bits".to_string()
        }));
    }

    Ok(key)
}

/// Encrypt data with RSA-OAEP-SHA256
pub fn encrypt_for_public_key(data: &[u8], public_key: &RsaPublicKey) -> Result<Vec<u8>> {
    let padding = Oaep::new::<Sha256>();
    let mut rng = rand::thread_rng();

    public_key.encrypt(&mut rng, padding, data).map_err(|e| {
        create_error!(EncryptionError {
            message: format!("RSA encryption failed: {e}")
        })
    })
}
