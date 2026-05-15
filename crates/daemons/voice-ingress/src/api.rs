use livekit_api::{access_token::TokenVerifier, webhooks::WebhookReceiver};
use livekit_protocol::TrackType;
use company_database::{
    analytics,
    events::client::EventV1,
    iso8601_timestamp::{Duration, Timestamp},
    util::reference::Reference,
    voice::{
        create_voice_state, create_viewer_state, delete_voice_state, delete_viewer_state,
        delete_voice_connection_type, get_voice_connection_type, get_viewer_count,
        get_voice_channel_members,
        get_user_moved_from_voice, get_user_moved_to_voice,
        update_voice_state_tracks, VoiceClient,
    },
    Database, AMQP,
};
use company_result::{Result, ToRevoltError};
use rocket::{post, State};
use rocket_empty::EmptyResponse;

use crate::guard::AuthHeader;

#[post("/<node>", data = "<body>")]
pub async fn ingress(
    db: &State<Database>,
    voice_client: &State<VoiceClient>,
    _amqp: &State<AMQP>,
    node: &str,
    auth_header: AuthHeader<'_>,
    body: &str,
) -> Result<EmptyResponse> {
    log::debug!("received event: {body:?}");

    let config = company_config::config().await;

    let node_info = config
        .api
        .livekit
        .nodes
        .get(node)
        .to_internal_error()
        .inspect_err(|_| {
            log::error!("Unknown node {node}, make sure livekit has the correct node name set and matches `hosts.livekit` and `api.livekit.nodes` in the Revolt config.")
        })?;

    let webhook_receiver = WebhookReceiver::new(TokenVerifier::with_api_key(
        &node_info.key,
        &node_info.secret,
    ));

    let event = webhook_receiver
        .receive(body, &auth_header)
        .to_internal_error()?;

    let channel_id = event.room.as_ref().map(|r| &r.name);
    let user_id = event.participant.as_ref().map(|r| &r.identity);

    match event.event.as_str() {
        // User joined a channel
        "participant_joined" => {
            let channel_id = channel_id.to_internal_error()?;
            let user_id = user_id.to_internal_error()?;

            let channel = Reference::from_unchecked(channel_id).as_channel(db).await?;

            // Check if this is a viewer or participant
            let connection_type = get_voice_connection_type(channel_id, user_id).await?;
            let is_viewer = connection_type.as_deref() == Some("viewer");

            if is_viewer {
                // Viewer joined — add to viewer set and broadcast count update
                create_viewer_state(channel_id, user_id).await?;

                let count = get_viewer_count(channel_id).await?;
                EventV1::ViewerCountUpdate {
                    id: channel_id.to_string(),
                    count,
                }
                .p(channel_id.to_string())
                .await;
            } else {
                // Participant joined — existing flow
                let joined_at = Timestamp::UNIX_EPOCH
                    .checked_add(Duration::seconds(event.created_at))
                    .unwrap();

                let voice_state =
                    create_voice_state(channel_id, channel.server(), user_id, joined_at).await?;

                // Only publish one event when a user is moved from one channel to another.
                if let Some(moved_from) = get_user_moved_to_voice(channel_id, user_id).await? {
                    EventV1::VoiceChannelMove {
                        user: user_id.to_string(),
                        from: moved_from,
                        to: channel_id.to_string(),
                        state: voice_state,
                    }
                    .p(channel_id.to_string())
                    .await;
                } else {
                    EventV1::VoiceChannelJoin {
                        id: channel_id.to_string(),
                        state: voice_state,
                    }
                    .p(channel_id.to_string())
                    .await;
                };
            }

            // --- Analytics tracking (join) ---
            // Wrapped in .ok() so analytics failures never break voice
            let _ = track_analytics_join(db, channel_id, user_id, channel.server()).await;
        }
        // User left a channel
        "participant_left" => {
            let channel_id = channel_id.to_internal_error()?;
            let user_id = user_id.to_internal_error()?;

            let channel = Reference::from_unchecked(channel_id).as_channel(db).await?;

            // Check if this is a viewer or participant
            let connection_type = get_voice_connection_type(channel_id, user_id).await?;
            let is_viewer = connection_type.as_deref() == Some("viewer");

            if is_viewer {
                // Viewer left — remove from viewer set, clean up connection type, broadcast count
                delete_viewer_state(channel_id, user_id).await?;
                delete_voice_connection_type(channel_id, user_id).await?;

                let count = get_viewer_count(channel_id).await?;
                EventV1::ViewerCountUpdate {
                    id: channel_id.to_string(),
                    count,
                }
                .p(channel_id.to_string())
                .await;
            } else {
                // Participant left — existing flow
                delete_voice_state(channel_id, channel.server(), user_id).await?;
                delete_voice_connection_type(channel_id, user_id).await?;

                // Dont send leave event when a user is moved
                if get_user_moved_from_voice(channel_id, user_id)
                    .await?
                    .is_none()
                {
                    EventV1::VoiceChannelLeave {
                        id: channel_id.clone(),
                        user: user_id.clone(),
                    }
                    .p(channel_id.clone())
                    .await;
                };
            }

            // --- Analytics tracking (leave) ---
            let _ = track_analytics_leave(db, channel_id).await;
        }
        // Audio/video track was started/stopped/unmuted/muted
        "track_published" | "track_unpublished" | "track_unmuted" | "track_muted" => {
            let channel_id = channel_id.to_internal_error()?;
            let user_id = user_id.to_internal_error()?;
            let track = event.track.as_ref().to_internal_error()?;

            // Viewers should not publish tracks — disconnect them if they do
            let connection_type = get_voice_connection_type(channel_id, user_id).await?;
            if connection_type.as_deref() == Some("viewer") && event.event == "track_published" {
                log::debug!("Viewer {user_id} attempted to publish track in {channel_id}, disconnecting.");
                let _ = voice_client.remove_user(node, user_id, channel_id).await;
                delete_viewer_state(channel_id, user_id).await?;
                delete_voice_connection_type(channel_id, user_id).await?;

                let count = get_viewer_count(channel_id).await?;
                EventV1::ViewerCountUpdate {
                    id: channel_id.to_string(),
                    count,
                }
                .p(channel_id.to_string())
                .await;

                return Ok(EmptyResponse);
            }

            let channel = Reference::from_unchecked(channel_id).as_channel(db).await?;

            let user = Reference::from_unchecked(user_id).as_user(db).await?;

            let user_limits = user.limits().await;

            // forbid any size which goes over the limit and also limit the aspect ratio to stop people from making too tall or too wide and bypassing the limit.
            // TODO: figure out how to track audio stream quality

            if event.event == "track_published" {
                let mut disconnect = false;

                if track.r#type == TrackType::Data as i32 {
                    log::debug!("User published data");
                    disconnect = true;
                };

                // Only apply resolution/aspect ratio limits to camera tracks (source 1),
                // not screenshare tracks (source 3/4) which are naturally high-resolution
                if track.r#type == TrackType::Video as i32 && track.source == 1 {
                    if user_limits.video_resolution[0] != 0
                        && user_limits.video_resolution[1] != 0
                        && track.width * track.height
                            > user_limits.video_resolution[0] * user_limits.video_resolution[1]
                    {
                        log::debug!("User published video with out of bounds resolution");
                        disconnect = true;
                    };

                    if user_limits.video_aspect_ratio[0] != user_limits.video_aspect_ratio[1]
                        && !(user_limits.video_aspect_ratio[0]..=user_limits.video_aspect_ratio[1])
                            .contains(&(track.width as f32 / track.height as f32))
                    {
                        log::debug!("User published video with out of bounds aspect ratio");
                        disconnect = true;
                    };
                };

                if disconnect {
                    log::debug!("Removing user {user_id} from channel {channel_id} {event:?} due to forbidden track.");

                    let _ = voice_client.remove_user(node, user_id, channel_id).await;
                    delete_voice_state(channel_id, channel.server(), user_id).await?;

                    return Ok(EmptyResponse);
                };
            };

            let partial = update_voice_state_tracks(
                channel_id,
                channel.server(),
                user_id,
                event.event == "track_published" || event.event == "track_unmuted", // to avoid duplicating this entire case twice
                track.source,
            )
            .await?;

            EventV1::UserVoiceStateUpdate {
                id: user_id.clone(),
                channel_id: channel_id.clone(),
                data: partial,
            }
            .p(channel_id.clone())
            .await;
        }
        _ => {}
    };

    Ok(EmptyResponse)
}

/// Track analytics when a user (viewer or participant) joins a voice channel.
/// Creates a new stream session if none exists, updates counts, records snapshot.
async fn track_analytics_join(
    db: &Database,
    channel_id: &str,
    user_id: &str,
    server_id: Option<&str>,
) -> Result<()> {
    // Phase 3: Only track analytics for public streams.
    // P2P voice has no server-side analytics (by design).
    let channel = Reference::from_unchecked(channel_id).as_channel(db).await;
    if let Ok(ch) = &channel {
        if !ch.voice().map_or(false, |v| v.is_stream_public) {
            return Ok(());
        }
    }

    // Get or create active session
    let session_id = match analytics::get_cached_session_id(channel_id).await {
        Some(id) => id,
        None => {
            // No cached session — create a new one
            let id = ulid::Ulid::new().to_string();
            analytics::create_stream_session(db, &id, channel_id, user_id, server_id).await?;
            analytics::cache_session_id(channel_id, &id).await;
            id
        }
    };

    // Get current counts from Redis
    let viewer_count = get_viewer_count(channel_id).await.unwrap_or(0);
    let participant_count = get_voice_channel_members(channel_id)
        .await
        .unwrap_or(None)
        .map(|m| m.len() as u32)
        .unwrap_or(0);

    // Update MongoDB counts and peaks
    analytics::update_stream_counts(db, &session_id, viewer_count, participant_count).await?;

    // Record snapshot for graphing
    analytics::record_snapshot(&session_id, viewer_count, participant_count).await;

    // Broadcast StreamMetricsUpdate
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    if let Ok(Some(session)) = analytics::get_active_session(db, channel_id).await {
        let duration_seconds = ((now_ms - session.started_at) / 1000).max(0) as u64;
        EventV1::StreamMetricsUpdate {
            channel: channel_id.to_string(),
            current_viewers: viewer_count,
            peak_viewers: session.peak_viewers.max(viewer_count),
            duration_seconds,
        }
        .p(channel_id.to_string())
        .await;
    }

    Ok(())
}

/// Track analytics when a user (viewer or participant) leaves a voice channel.
/// Updates counts, checks if channel is empty and ends session if so.
async fn track_analytics_leave(db: &Database, channel_id: &str) -> Result<()> {
    // Phase 3: Only track analytics for public streams.
    // P2P voice has no server-side analytics (by design).
    let channel = Reference::from_unchecked(channel_id).as_channel(db).await;
    if let Ok(ch) = &channel {
        if !ch.voice().map_or(false, |v| v.is_stream_public) {
            return Ok(());
        }
    }

    let session_id = match analytics::get_cached_session_id(channel_id).await {
        Some(id) => id,
        None => return Ok(()), // No active session, nothing to track
    };

    // Get current counts after the leave has been processed
    let viewer_count = get_viewer_count(channel_id).await.unwrap_or(0);
    let participant_count = get_voice_channel_members(channel_id)
        .await
        .unwrap_or(None)
        .map(|m| m.len() as u32)
        .unwrap_or(0);

    // Check if channel is now empty
    if viewer_count == 0 && participant_count == 0 {
        // End the stream session
        analytics::end_stream_session(db, &session_id).await?;
        analytics::cleanup_redis(channel_id, &session_id).await;
    } else {
        // Update counts
        analytics::update_stream_counts(db, &session_id, viewer_count, participant_count).await?;
        analytics::record_snapshot(&session_id, viewer_count, participant_count).await;

        // Broadcast update
        if let Ok(Some(session)) = analytics::get_active_session(db, channel_id).await {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let duration_seconds = ((now_ms - session.started_at) / 1000).max(0) as u64;
            EventV1::StreamMetricsUpdate {
                channel: channel_id.to_string(),
                current_viewers: viewer_count,
                peak_viewers: session.peak_viewers,
                duration_seconds,
            }
            .p(channel_id.to_string())
            .await;
        }
    }

    Ok(())
}
