use company_database::{
    analytics,
    util::{permissions::perms, reference::Reference},
    Database, User,
};
use company_models::v0::CurrentStreamMetrics;
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};

use rocket::{serde::json::Json, State};

/// # Get Current Stream Analytics
///
/// Get real-time analytics for the active stream on a voice channel.
/// Requires ManageChannel permission.
#[openapi(tag = "Voice")]
#[get("/<target>/analytics/current")]
pub async fn get_current_analytics(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
) -> Result<Json<CurrentStreamMetrics>> {
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

    let metrics = analytics::get_current_metrics(db, channel.id()).await?;
    Ok(Json(metrics))
}
