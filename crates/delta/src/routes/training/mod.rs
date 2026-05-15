pub mod consent;
pub mod encryption;
pub mod public_key;
pub mod submission;

pub use encryption::{DatabaseEncryption, TrainingKeyPair};

use rocket::Route;

/// Configuration for the training data pipeline, loaded from Revolt.toml.
#[derive(Debug, Clone)]
pub struct TrainingConfig {
    pub private_key_path: String,
    pub public_key_path: String,
    pub db_master_key: String,
    pub jwt_secret: String,
}

/// Training routes are NOT part of the OpenAPI spec (no user auth).
/// Mounted separately at /api/training.
pub fn training_routes() -> Vec<Route> {
    routes![submission::submit_training_data, public_key::get_public_key,]
}
