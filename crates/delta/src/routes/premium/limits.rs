use company_database::User;
use company_result::Result;
use rocket::serde::json::Json;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct LimitsResponse {
    /// Current subscription tier
    tier: String,
    /// Allowed clip durations in seconds
    clip_durations: Vec<u32>,
    /// Maximum clips per day (null = unlimited)
    clips_per_day: Option<u32>,
    /// Maximum total clips stored (null = unlimited)
    max_clips_stored: Option<u32>,
    /// Maximum clip storage in GB (null = unlimited)
    max_clip_storage_gb: Option<u32>,
    /// Whether VOD recording is enabled
    vod_enabled: bool,
    /// Whether clips can be downloaded
    can_download_clips: bool,
    /// Whether clipping can be enabled on streams
    can_enable_clipping: bool,
    /// Maximum custom emoji slots
    max_emoji_slots: u32,
    /// Maximum file upload size in bytes
    max_upload_bytes: usize,
    /// Maximum servers the user can create/join
    max_servers: usize,
    /// Maximum voice bitrate in bps
    max_voice_bitrate: u32,
    /// Maximum stream viewers (0 = unlimited)
    max_stream_viewers: usize,
    /// Whether LiveKit SFU is used for public streams
    uses_livekit: bool,
    /// Whether stream recording is enabled
    stream_recording: bool,
    /// Whether priority routing is enabled
    priority_routing: bool,
}

/// # Get Usage Limits
///
/// Get the current user's tier-based feature limits.
#[openapi(tag = "Premium")]
#[get("/limits")]
pub async fn get_limits(user: User) -> Result<Json<LimitsResponse>> {
    let tier = user.active_subscription_tier();
    let limits = user.limits().await;

    Ok(Json(LimitsResponse {
        tier: format!("{:?}", tier),
        clip_durations: tier.clip_durations_seconds().to_vec(),
        clips_per_day: tier.clips_per_day(),
        max_clips_stored: tier.max_clips_stored(),
        max_clip_storage_gb: tier.max_clip_storage_gb(),
        vod_enabled: tier.vod_recording_enabled(),
        can_download_clips: tier.can_download_clips(),
        can_enable_clipping: tier.can_enable_clipping(),
        max_emoji_slots: tier.max_emoji_slots(),
        max_upload_bytes: limits.file_upload_size_limit.get("attachments").copied().unwrap_or(0),
        max_servers: limits.servers,
        max_voice_bitrate: limits.max_voice_bitrate,
        max_stream_viewers: tier.stream_viewer_limit(),
        uses_livekit: tier.uses_livekit_for_public_streams(),
        stream_recording: tier.stream_recording_enabled(),
        priority_routing: tier.priority_stream_routing(),
    }))
}
