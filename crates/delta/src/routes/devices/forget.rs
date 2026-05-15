use authifier::models::Session;
use authifier::Authifier;
use company_database::broker;
use company_result::{create_error, Result};
use rocket::State;
use rocket_empty::EmptyResponse;

/// # Forget Device
///
/// Forcibly remove a paired device from the broker's device registry,
/// dropping its offline message queue and any pending sync envelopes.
///
/// The caller must be authenticated as the same user that owns the target
/// device. Forgetting a device that is currently online will cause it to be
/// dropped from the registry — it will need to reconnect and re-register
/// before it can receive offline messages again.
///
/// Note: this does NOT revoke the device's authifier session token. Use
/// the existing `/auth/session/{id}` DELETE endpoint to also revoke login.
#[openapi(tag = "Devices")]
#[delete("/<target_session_id>")]
pub async fn forget(
    authifier: &State<Authifier>,
    caller: Session,
    target_session_id: String,
) -> Result<EmptyResponse> {
    // Verify the target session belongs to the caller.
    let target = authifier
        .database
        .find_session(&target_session_id)
        .await
        .map_err(|_| create_error!(NotFound))?;

    if target.user_id != caller.user_id {
        return Err(create_error!(NotFound));
    }

    broker::forget_device(&caller.user_id, &target_session_id)
        .await
        .map(|_| EmptyResponse)
        .map_err(|_| create_error!(InternalError))
}
