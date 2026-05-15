use authifier::models::Session;
use company_database::broker;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use serde::Serialize;

/// One device session in the list response. This mirrors
/// `broker::DeviceInfo` but adds `JsonSchema` for OpenAPI generation.
#[derive(Serialize, schemars::JsonSchema)]
pub struct DeviceListItem {
    pub session_id: String,
    pub node_id: String,
    pub connected_at: String,
    pub last_seen_unix: i64,
    pub is_online: bool,
}

impl From<broker::DeviceInfo> for DeviceListItem {
    fn from(d: broker::DeviceInfo) -> Self {
        Self {
            session_id: d.session_id,
            node_id: d.node_id,
            connected_at: d.connected_at,
            last_seen_unix: d.last_seen_unix,
            is_online: d.is_online,
        }
    }
}

/// # List Devices
///
/// List all messaging-capable device sessions registered for the
/// authenticated user. Used by the Devices settings tab to populate the
/// list of paired devices, including last-seen and online status.
#[openapi(tag = "Devices")]
#[get("/")]
pub async fn list(session: Session) -> Result<Json<Vec<DeviceListItem>>> {
    let devices = broker::list_user_devices(&session.user_id)
        .await
        .map_err(|_| create_error!(InternalError))?;
    Ok(Json(devices.into_iter().map(Into::into).collect()))
}
