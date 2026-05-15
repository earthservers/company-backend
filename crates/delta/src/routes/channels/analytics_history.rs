use company_database::{
    analytics,
    util::{permissions::perms, reference::Reference},
    Database, User,
};
use company_models::v0::StreamHistoryResponse;
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};
use serde::Deserialize;

use rocket::{serde::json::Json, State};

/// Query parameters for stream history endpoint
#[derive(Deserialize, JsonSchema, FromForm)]
pub struct HistoryParams {
    /// Number of days of history to return (default: 30, max: 365)
    pub days: Option<u32>,
}

/// # Get Stream History
///
/// Get historical stream session summaries for a voice channel.
/// Requires ManageChannel permission.
#[openapi(tag = "Voice")]
#[get("/<target>/analytics/history?<params..>")]
pub async fn get_history(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
    params: HistoryParams,
) -> Result<Json<StreamHistoryResponse>> {
    let channel = target.as_channel(db).await?;

    // Must be a voice channel
    if channel.voice().is_none() {
        return Err(create_error!(NotAVoiceChannel));
    }

    // Permission check: must have ManageChannel
    let mut permissions = perms(db, &user).channel(&channel);
    let perm_value = calculate_channel_permissions(&mut permissions).await;
    if !perm_value.has(ChannelPermission::ManageChannel as u64) {
        return Err(create_error!(MissingPermission {
            permission: "ManageChannel".to_string()
        }));
    }

    let days = params.days.unwrap_or(30).min(365);
    let sessions = analytics::get_stream_history(db, channel.id(), days).await?;

    Ok(Json(StreamHistoryResponse { sessions }))
}
