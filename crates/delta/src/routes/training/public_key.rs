use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::State;
use serde::Serialize;

use super::encryption::TrainingKeyPair;

#[derive(Serialize)]
pub struct PublicKeyResponse {
    pub public_key_pem: String,
}

/// GET /api/training/public-key
///
/// Returns the server's RSA public key so clients can encrypt training data
/// before submitting it.
#[get("/public-key")]
pub fn get_public_key(
    training_keys: &State<Option<TrainingKeyPair>>,
) -> Result<Json<PublicKeyResponse>, Status> {
    let keys = training_keys.as_ref().ok_or_else(|| {
        log::error!("Training keys not configured");
        Status::ServiceUnavailable
    })?;

    let public_key_pem = keys
        .public_key_pem()
        .map_err(|_| Status::InternalServerError)?;

    Ok(Json(PublicKeyResponse { public_key_pem }))
}
