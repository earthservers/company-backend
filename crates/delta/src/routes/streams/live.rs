use company_database::{
    util::reference::Reference,
    voice::{get_active_voice_channel_ids, get_voice_channel_members, get_voice_state, get_viewer_count},
    Database, User,
};
use company_result::Result;
use rocket::{serde::json::Json, State};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct LiveStream {
    /// Voice channel ID
    pub channel_id: String,
    /// Server ID (if this is a server channel)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// Server name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    /// Server icon file ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_icon: Option<String>,
    /// User ID of the person streaming
    pub streamer_id: String,
    /// Display name of the streamer
    pub streamer_name: String,
    /// Avatar file ID of the streamer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamer_avatar: Option<String>,
    /// Number of viewers
    pub viewer_count: u32,
    /// Number of voice participants
    pub participant_count: u32,
    /// Stream start time (not yet tracked)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    /// Channel description / stream title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LiveStreamsResponse {
    /// List of currently active public streams
    pub streams: Vec<LiveStream>,
}

/// # Get Live Streams
///
/// Returns currently active public streams across all servers.
#[openapi(tag = "Streams")]
#[get("/live")]
pub async fn get_live_streams(
    db: &State<Database>,
    _user: User,
) -> Result<Json<LiveStreamsResponse>> {
    let channel_ids = get_active_voice_channel_ids().await?;
    let mut streams = Vec::new();

    for channel_id in channel_ids {
        // Fetch channel
        let Ok(channel) = Reference::from_unchecked(&channel_id).as_channel(db).await else {
            continue;
        };

        // Check if this is a public stream
        let voice_info = match channel.voice() {
            Some(v) if v.is_stream_public => v,
            _ => continue,
        };
        drop(voice_info);

        // Get members
        let members = match get_voice_channel_members(&channel_id).await? {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };

        // Find the first member who is screensharing or has camera on
        let mut streamer_id = None;
        for user_id in &members {
            if let Ok(Some(state)) = get_voice_state(&channel_id, channel.server(), user_id).await {
                if state.screensharing || state.camera {
                    streamer_id = Some(user_id.clone());
                    break;
                }
            }
        }

        // If nobody is actually streaming video/screen, skip
        let streamer_id = match streamer_id {
            Some(id) => id,
            None => continue,
        };

        // Fetch streamer user info
        let Ok(streamer) = Reference::from_unchecked(&streamer_id).as_user(db).await else {
            continue;
        };

        // Fetch server info if applicable
        let (server_id, server_name, server_icon) = if let Some(sid) = channel.server() {
            match db.fetch_server(sid).await {
                Ok(server) => (
                    Some(server.id.clone()),
                    Some(server.name.clone()),
                    server.icon.as_ref().map(|f| f.id.clone()),
                ),
                Err(_) => (Some(sid.to_string()), None, None),
            }
        } else {
            (None, None, None)
        };

        let viewer_count = get_viewer_count(&channel_id).await.unwrap_or(0);

        // Get channel description as stream title
        let title = match &channel {
            company_database::Channel::TextChannel { description, .. } => description.clone(),
            company_database::Channel::Group { description, .. } => description.clone(),
            _ => None,
        };

        streams.push(LiveStream {
            channel_id: channel_id.clone(),
            server_id,
            server_name,
            server_icon,
            streamer_id: streamer.id.clone(),
            streamer_name: streamer
                .display_name
                .clone()
                .unwrap_or_else(|| streamer.username.clone()),
            streamer_avatar: streamer.avatar.as_ref().map(|f| f.id.clone()),
            viewer_count,
            participant_count: members.len() as u32,
            started_at: None,
            title,
        });
    }

    // Sort by viewer count descending
    streams.sort_by(|a, b| b.viewer_count.cmp(&a.viewer_count));

    Ok(Json(LiveStreamsResponse { streams }))
}
