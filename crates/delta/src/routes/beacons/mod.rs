use company_database::{Beacon, BeaconExpiryMode, BeaconStatus};
use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;
use schemars::JsonSchema;
use serde::Serialize;

mod delete;
mod list;
mod register;
mod seen;
mod update;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        register::register_beacon,
        list::list_my_beacons,
        update::update_beacon,
        delete::delete_beacon,
        seen::record_beacon_seen,
    ]
}

/// Wire form for a single beacon record.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BeaconResponse {
    /// Composite id: `{owner_user_id}:{file_hash}`.
    pub id: String,
    pub owner_user_id: String,
    pub file_hash: String,
    pub status: String,
    pub expiry_mode: String,
    /// ISO 8601 string, present when `expiry_mode == Custom`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub auto_beacon: bool,
    pub download_count: u64,
    /// ISO 8601 string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_download_at: Option<String>,
    pub created_at: String,
}

impl From<Beacon> for BeaconResponse {
    fn from(b: Beacon) -> Self {
        BeaconResponse {
            id: b.id,
            owner_user_id: b.owner_user_id,
            file_hash: b.file_hash,
            status: match b.status {
                BeaconStatus::Active => "Active",
                BeaconStatus::Deleted => "Deleted",
                BeaconStatus::Expired => "Expired",
            }
            .to_string(),
            expiry_mode: match b.expiry_mode {
                BeaconExpiryMode::Ephemeral => "Ephemeral",
                BeaconExpiryMode::AlwaysOnline => "AlwaysOnline",
                BeaconExpiryMode::Custom => "Custom",
            }
            .to_string(),
            expires_at: b.expires_at.map(|t| t.to_string()),
            auto_beacon: b.auto_beacon,
            download_count: b.download_count,
            last_download_at: b.last_download_at.map(|t| t.to_string()),
            created_at: b.created_at.to_string(),
        }
    }
}
