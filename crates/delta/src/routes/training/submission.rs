use company_database::mongodb::bson::{doc, Binary};
use company_database::Database;
use rocket::data::{Data, ToByteUnit};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};

use super::consent::{validate_training_jwt, verify_device_attestation};
use super::encryption::{DatabaseEncryption, TrainingKeyPair};
use super::TrainingConfig;

/// Request guard that extracts the Authorization Bearer token.
pub struct BearerToken(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BearerToken {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("Authorization") {
            Some(header) => match header.strip_prefix("Bearer ") {
                Some(token) => Outcome::Success(BearerToken(token.to_string())),
                None => Outcome::Error((Status::Unauthorized, ())),
            },
            None => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

#[derive(Deserialize)]
pub struct TrainingSubmission {
    /// Hex-encoded RSA-encrypted training data
    pub encrypted_data: String,
    pub metadata: TrainingMetadata,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct TrainingMetadata {
    pub anonymized_user: String,
    pub timestamp: String,
    pub channel_type: String,
    pub message_length: usize,
}

#[derive(Serialize)]
pub struct SubmissionResponse {
    pub success: bool,
    pub message: String,
}

/// POST /api/training/submit
///
/// Receives encrypted training data from AI Companion clients:
/// 1. Validates JWT (proof of consent + device attestation)
/// 2. Decrypts with server private key (Layer 1: RSA-OAEP-SHA256)
/// 3. Re-encrypts with AES-256-GCM for storage (Layer 2)
/// 4. Stores in training database (double encrypted at rest)
#[post("/submit", data = "<body>")]
pub async fn submit_training_data(
    body: Data<'_>,
    token: BearerToken,
    training_keys: &State<Option<TrainingKeyPair>>,
    db_encryption: &State<Option<DatabaseEncryption>>,
    db: &State<Database>,
    training_config: &State<TrainingConfig>,
) -> Result<Json<SubmissionResponse>, Status> {
    // Ensure training pipeline is configured
    let training_keys = training_keys.as_ref().ok_or_else(|| {
        log::error!("Training keys not configured");
        Status::ServiceUnavailable
    })?;
    let db_encryption = db_encryption.as_ref().ok_or_else(|| {
        log::error!("Training DB encryption not configured");
        Status::ServiceUnavailable
    })?;

    // Read raw body (limit to 1 MB)
    let body_bytes = match body.open(1.mebibytes()).into_bytes().await {
        Ok(bytes) if bytes.is_complete() => bytes.into_inner(),
        _ => return Err(Status::PayloadTooLarge),
    };

    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => return Err(Status::BadRequest),
    };

    let submission: TrainingSubmission = match serde_json::from_str(body_str) {
        Ok(s) => s,
        Err(_) => return Err(Status::BadRequest),
    };

    // 1. Validate JWT
    let claims = match validate_training_jwt(&token.0, &training_config.jwt_secret) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Training submit: invalid JWT: {e}");
            return Ok(Json(SubmissionResponse {
                success: false,
                message: "Invalid training consent token".to_string(),
            }));
        }
    };

    log::info!(
        "Training submit: JWT validated for anonymized user {}",
        submission.metadata.anonymized_user
    );

    // 2. Verify device attestation
    if !verify_device_attestation(
        &claims.device_attestation,
        &submission.metadata.anonymized_user,
    ) {
        return Ok(Json(SubmissionResponse {
            success: false,
            message: "Device attestation failed".to_string(),
        }));
    }

    // 3. Decrypt from client (Layer 1: RSA)
    let encrypted_bytes = match hex::decode(&submission.encrypted_data) {
        Ok(b) => b,
        Err(_) => {
            return Ok(Json(SubmissionResponse {
                success: false,
                message: "Invalid data format".to_string(),
            }));
        }
    };

    let plaintext = match training_keys.decrypt_from_client(&encrypted_bytes) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Training submit: RSA decryption failed: {e}");
            return Ok(Json(SubmissionResponse {
                success: false,
                message: "Failed to decrypt training data".to_string(),
            }));
        }
    };

    log::info!(
        "Training submit: Layer 1 decrypted ({} bytes)",
        plaintext.len()
    );

    // 4. Re-encrypt for storage (Layer 2: AES-256-GCM)
    let storage_encrypted = match db_encryption.encrypt_for_storage(&plaintext) {
        Ok(e) => e,
        Err(e) => {
            log::error!("Training submit: storage encryption failed: {e}");
            return Ok(Json(SubmissionResponse {
                success: false,
                message: "Storage encryption error".to_string(),
            }));
        }
    };

    log::info!(
        "Training submit: Layer 2 encrypted ({} bytes)",
        storage_encrypted.len()
    );

    // 5. Store in database (double encrypted at rest)
    let mongo = db.mongodb();
    let col = mongo.col::<company_database::mongodb::bson::Document>("training_encrypted_messages");

    let document = doc! {
        "encrypted_content": Binary {
            subtype: company_database::mongodb::bson::spec::BinarySubtype::Generic,
            bytes: storage_encrypted,
        },
        "metadata": company_database::mongodb::bson::to_bson(&submission.metadata)
            .unwrap_or_default(),
        "device_attestation_method": &claims.device_attestation.method,
        "received_at": chrono::Utc::now().to_rfc3339(),
        "jwt_timestamp": claims.timestamp,
    };

    match col.insert_one(document).await {
        Ok(result) => {
            log::info!(
                "Training submit: stored (double encrypted) - ID: {:?}",
                result.inserted_id
            );
            Ok(Json(SubmissionResponse {
                success: true,
                message: "Training data received and stored securely".to_string(),
            }))
        }
        Err(e) => {
            log::error!("Training submit: database insert failed: {e}");
            Ok(Json(SubmissionResponse {
                success: false,
                message: "Failed to store training data".to_string(),
            }))
        }
    }
}
