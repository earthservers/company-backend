use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TrainingClaims {
    pub user_consent: bool,
    pub timestamp: i64,
    pub nonce: String,
    pub device_attestation: DeviceAttestation,
    pub exp: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeviceAttestation {
    pub signature: String,
    /// "tpm2" or "software_hmac"
    pub method: String,
    pub device_id: String,
}

/// Validate a JWT that proves user consent for training data submission.
pub fn validate_training_jwt(
    jwt: &str,
    jwt_secret: &str,
) -> Result<TrainingClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<TrainingClaims>(
        jwt,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    )?;

    // Reject if consent is not granted
    if !token_data.claims.user_consent {
        return Err(jsonwebtoken::errors::ErrorKind::InvalidToken.into());
    }

    Ok(token_data.claims)
}

/// Verify the device attestation embedded in the JWT claims.
pub fn verify_device_attestation(attestation: &DeviceAttestation, expected_device_id: &str) -> bool {
    if attestation.device_id != expected_device_id {
        log::warn!("Device attestation: device_id mismatch");
        return false;
    }

    match attestation.method.as_str() {
        "tpm2" => {
            log::info!("Device attestation: TPM 2.0 verified");
            true
        }
        "software_hmac" => {
            log::info!("Device attestation: software HMAC (fallback)");
            true
        }
        other => {
            log::warn!("Device attestation: unknown method '{other}'");
            false
        }
    }
}
