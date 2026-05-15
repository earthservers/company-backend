use authifier::models::Session;
use company_database::broker;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use serde::Serialize;

#[derive(Serialize, schemars::JsonSchema)]
pub struct SyncDrainResponse {
    /// Encrypted sync envelopes addressed to the caller's device. Each is
    /// opaque to the server; the caller decrypts each one locally.
    /// Returned in FIFO order (oldest first).
    pub envelopes: Vec<String>,
}

/// # Drain Sync Envelopes
///
/// Drain all sync envelopes currently queued for the caller's device.
/// Returns them in the order they were pushed and deletes the queue
/// atomically — a second drain immediately after will return an empty list.
///
/// Clients should call this:
///   - Once on connect / app foreground
///   - After the user manually triggers a "Sync from another device" flow
#[openapi(tag = "Devices")]
#[get("/sync/drain")]
pub async fn sync_drain(caller: Session) -> Result<Json<SyncDrainResponse>> {
    let envelopes = broker::drain_sync_envelopes(&caller.user_id, &caller.id)
        .await
        .map_err(|_| create_error!(InternalError))?;
    Ok(Json(SyncDrainResponse { envelopes }))
}
