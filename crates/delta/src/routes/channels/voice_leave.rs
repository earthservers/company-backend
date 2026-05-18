use company_database::{
    events::client::EventV1,
    util::reference::Reference,
    voice::{
        delete_viewer_state, delete_voice_connection_type, delete_voice_state,
        get_channel_node, get_voice_connection_type, get_voice_state, VoiceClient,
    },
    Database, User,
};
use company_result::{create_error, Result};

use rocket::State;
use rocket_empty::EmptyResponse;

/// # Leave Call
///
/// Tear down the user's voice state for this channel. For P2P calls
/// there is no LiveKit webhook to drive cleanup, so the client must
/// call this endpoint on disconnect — otherwise vc_members and the
/// user's voice channel set go stale and the sidebar keeps showing
/// the user as in-call.
///
/// For LiveKit calls we also ask the SFU to evict the user; the
/// participant_left webhook will run the same teardown, but doing it
/// here keeps the leave snappy and idempotent (everything below is
/// safe to run twice).
///
/// Returns NoEffect if the user wasn't in the channel's voice state.
#[openapi(tag = "Voice")]
#[post("/<target>/leave_call")]
pub async fn leave_call(
    db: &State<Database>,
    voice_client: &State<VoiceClient>,
    user: User,
    target: Reference<'_>,
) -> Result<EmptyResponse> {
    let channel = target.as_channel(db).await?;

    // No-op if the user isn't actually in this channel's voice state.
    // We don't want to emit a phantom Leave event for a user who was
    // never in the room.
    if get_voice_state(channel.id(), channel.server(), &user.id)
        .await?
        .is_none()
    {
        return Err(create_error!(NoEffect));
    }

    let connection_type = get_voice_connection_type(channel.id(), &user.id).await?;
    let is_viewer = connection_type.as_deref() == Some("viewer");

    // For LiveKit-backed channels, ask the SFU to evict the user.
    // For P2P there is no node — peer connections tear down client-side
    // when P2PVoice.disconnect() sends its "leave" signal.
    if let Some(node) = get_channel_node(channel.id()).await? {
        let _ = voice_client
            .remove_user(&node, &user.id, channel.id())
            .await;
    }

    if is_viewer {
        delete_viewer_state(channel.id(), &user.id).await?;
        delete_voice_connection_type(channel.id(), &user.id).await?;
    } else {
        delete_voice_state(channel.id(), channel.server(), &user.id).await?;
        delete_voice_connection_type(channel.id(), &user.id).await?;

        EventV1::VoiceChannelLeave {
            id: channel.id().to_string(),
            user: user.id.clone(),
        }
        .p(channel.id().to_string())
        .await;
    }

    Ok(EmptyResponse)
}
