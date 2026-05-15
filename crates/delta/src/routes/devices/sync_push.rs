use authifier::models::Session;
use authifier::Authifier;
use company_database::broker;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use rocket_empty::EmptyResponse;
use serde::Deserialize;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SyncPushBody {
    /// Authifier session ID of the destination device. Must belong to the
    /// same user as the caller (verified by this endpoint).
    pub dest_session_id: String,

    /// One encrypted sync envelope. Opaque to the server — the destination
    /// device decrypts it locally with its own keys.
    pub envelope: String,
}

/// # Push Sync Envelope
///
/// Push one encrypted sync envelope from the caller's device to another
/// of the user's paired devices. The envelope contains a batch of historical
/// messages (and mutations like edits, deletes, reactions) that the
/// destination device is missing.
///
/// The server enforces:
///   - Both source and destination sessions belong to the same user
///   - Envelope size limit (64 KB)
///   - Per-source rate limit (10 pushes per hour across all destinations)
///   - Per-destination queue depth limit (100 envelopes)
///
/// The destination device drains its queue via `GET /devices/sync/drain`
/// the next time it polls or reconnects.
#[openapi(tag = "Devices")]
#[post("/sync/push", data = "<body>")]
pub async fn sync_push(
    authifier: &State<Authifier>,
    caller: Session,
    body: Json<SyncPushBody>,
) -> Result<EmptyResponse> {
    let body = body.into_inner();

    // Refuse to push to yourself — that's a no-op and would inflate the
    // rate limit counter for nothing.
    if body.dest_session_id == caller.id {
        return Err(create_error!(FailedValidation {
            error: "Cannot sync from a device to itself.".to_string()
        }));
    }

    // Verify the destination session belongs to the caller's user.
    let dest = authifier
        .database
        .find_session(&body.dest_session_id)
        .await
        .map_err(|_| create_error!(NotFound))?;

    if dest.user_id != caller.user_id {
        return Err(create_error!(NotFound));
    }

    broker::push_sync_envelope(
        &caller.user_id,
        &caller.id,
        &body.dest_session_id,
        &body.envelope,
    )
    .await
    .map(|_| EmptyResponse)
}
