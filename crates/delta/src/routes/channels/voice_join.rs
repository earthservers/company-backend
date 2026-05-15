use company_config::config;
use company_database::{
    util::{permissions::perms, reference::Reference},
    voice::{
        delete_voice_state, get_channel_node, get_user_voice_channels, get_viewer_count,
        get_voice_channel_members, raise_if_in_voice, set_call_notification_recipients,
        set_voice_connection_type, VoiceClient,
    },
    Database, User,
};
use company_models::v0;
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};

use rocket::{serde::json::Json, State};

/// # Join Call
///
/// Asks the voice server for a token to join the call.
/// Users with Connect permission join as participants.
/// Users without Connect permission can join as viewers if the channel has public streaming enabled.
#[openapi(tag = "Voice")]
#[post("/<target>/join_call", data = "<data>")]
pub async fn call(
    db: &State<Database>,
    voice_client: &State<VoiceClient>,
    user: User,
    target: Reference<'_>,
    data: Json<v0::DataJoinCall>,
) -> Result<Json<v0::CreateVoiceUserResponse>> {
    if !voice_client.is_enabled() {
        return Err(create_error!(LiveKitUnavailable));
    }

    let v0::DataJoinCall {
        node,
        force_disconnect,
        recipients,
    } = data.into_inner();

    if user.bot.is_some() && force_disconnect == Some(true) {
        return Err(create_error!(IsBot));
    }

    let channel = target.as_channel(db).await?;

    let Some(voice_info) = channel.voice() else {
        return Err(create_error!(NotAVoiceChannel));
    };

    let mut permissions = perms(db, &user).channel(&channel);

    let current_permissions = calculate_channel_permissions(&mut permissions).await;
    let has_connect = current_permissions.has_channel_permission(ChannelPermission::Connect);

    // Determine connection type: participant (has Connect) or viewer (public stream, no Connect)
    let is_viewer = if has_connect {
        false
    } else if voice_info.is_stream_public {
        true
    } else {
        return Err(create_error!(MissingPermission {
            permission: "Connect".to_string()
        }));
    };

    // Only enforce max_users for participants, not viewers
    if !is_viewer {
        if get_voice_channel_members(channel.id())
            .await?
            .zip(voice_info.max_users)
            .is_some_and(|(ms, max_users)| ms.len() >= max_users)
            && !current_permissions.has(ChannelPermission::ManageChannel as u64)
        {
            return Err(create_error!(CannotJoinCall));
        }
    }

    // Enforce viewer limits based on streamer's subscription tier
    if is_viewer {
        if let Some(server_id) = channel.server() {
            let server = db.fetch_server(server_id).await?;
            let owner = db.fetch_user(&server.owner).await?;
            let tier = owner.active_subscription_tier();
            let max_viewers = tier.stream_viewer_limit();

            // 0 = unlimited (Ultra tier)
            if max_viewers > 0 {
                let current_viewers = get_viewer_count(channel.id()).await? as usize;
                if current_viewers >= max_viewers {
                    let tier_name = format!("{:?}", tier);
                    let upgrade_hint = match tier {
                        company_database::SubscriptionTier::Free => "Upgrade to Basic ($5/mo) for 50 viewers with LiveKit SFU.",
                        company_database::SubscriptionTier::Basic => "Upgrade to Pro ($10/mo) for 200 viewers + stream recording.",
                        company_database::SubscriptionTier::Pro => "Upgrade to Ultra ($20/mo) for unlimited viewers + priority routing.",
                        company_database::SubscriptionTier::Ultra => "Contact support for higher limits.",
                    };
                    return Err(create_error!(ViewerLimitReached {
                        max: max_viewers,
                        message: format!(
                            "This stream has reached its {} tier limit of {} viewers. {}",
                            tier_name, max_viewers, upgrade_hint
                        )
                    }));
                }
            }
        }
    }

    // Phase 3: Route voice connections based on channel type.
    // - Public streams (is_stream_public && paid tier): use LiveKit SFU
    // - Everything else (private voice, DM calls, group calls): P2P only
    let use_livekit = voice_info.is_stream_public
        && {
            // Check streamer's (server owner's) tier.
            // Reuse the server/owner fetch from viewer limit checks when possible.
            if let Some(server_id) = channel.server() {
                let server = db.fetch_server(server_id).await?;
                let owner = db.fetch_user(&server.owner).await?;
                owner.active_subscription_tier().uses_livekit_for_public_streams()
            } else {
                false
            }
        };

    if !use_livekit {
        // P2P voice: return signaling-only response (no LiveKit token).
        // Client uses P2PVoice.ts with WebRTC signaling via WebSocket.
        let connection_type = if is_viewer { "viewer" } else { "participant" };
        set_voice_connection_type(channel.id(), &user.id, connection_type).await?;
        return Ok(Json(v0::CreateVoiceUserResponse {
            token: String::new(),
            url: String::new(),
            connection_type: "p2p".to_string(),
        }));
    }

    let existing_node = get_channel_node(channel.id()).await?;

    let node = existing_node
        .or(node)
        .ok_or_else(|| create_error!(UnknownNode))?;

    let config = config().await;

    let node_host = config
        .hosts
        .livekit
        .get(&node)
        .ok_or_else(|| create_error!(UnknownNode))?
        .clone();

    if force_disconnect == Some(true) {
        // Finds and disconnects any existing voice connections by the user,
        // should only ever loop once but just to cover our backs.

        for channel_id in get_user_voice_channels(&user.id).await? {
            if let Some(node) = get_channel_node(&channel_id).await? {
                // if this errors its just a mismatching state - ignore and proceed to still delete our state
                let _ = voice_client.remove_user(&node, &user.id, &channel_id).await;
            };

            let channel = Reference::from_unchecked(&channel_id)
                .as_channel(db)
                .await?;

            delete_voice_state(&channel_id, channel.server(), &user.id).await?;
        }
    } else {
        raise_if_in_voice(&user, channel.id()).await?;
    }

    // Generate token based on connection type
    let token = if is_viewer {
        voice_client
            .create_viewer_token(&node, db, &user, &channel)
            .await?
    } else {
        voice_client
            .create_token(&node, db, &user, current_permissions, &channel)
            .await?
    };

    let room = voice_client.create_room(&node, &channel).await?;

    log::debug!("Created room {}", room.name);

    // Store connection type in Redis for the voice-ingress daemon
    let connection_type = if is_viewer { "viewer" } else { "participant" };
    set_voice_connection_type(channel.id(), &user.id, connection_type).await?;

    // Only set call notification recipients for participants, not viewers
    if !is_viewer {
        if let Some(recipients) = recipients {
            if room.num_participants == 0 && !recipients.is_empty() {
                set_call_notification_recipients(channel.id(), &user.id, &recipients).await?;
            }
        }
    }

    Ok(Json(v0::CreateVoiceUserResponse {
        token,
        url: node_host.clone(),
        connection_type: connection_type.to_string(),
    }))
}
