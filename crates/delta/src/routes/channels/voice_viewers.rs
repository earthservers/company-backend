use company_database::{
    util::{permissions::perms, reference::Reference},
    voice::{get_voice_connection_type, get_viewer_count, get_viewer_list},
    Database, User, ViewerListVisibility,
};
use company_models::v0;
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};

use rocket::{serde::json::Json, State};

/// # Get Viewers
///
/// Get the viewer count and optionally the viewer list for a voice channel with public streaming.
#[openapi(tag = "Voice")]
#[get("/<target>/viewers")]
pub async fn get_viewers(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
) -> Result<Json<v0::ViewersResponse>> {
    let channel = target.as_channel(db).await?;

    let Some(voice_info) = channel.voice() else {
        return Err(create_error!(NotAVoiceChannel));
    };

    if !voice_info.is_stream_public {
        return Ok(Json(v0::ViewersResponse {
            count: 0,
            viewers: None,
        }));
    }

    let count = get_viewer_count(channel.id()).await?;

    // Determine if the requester can see the viewer list
    let can_see_list = match voice_info.show_viewer_list_to {
        ViewerListVisibility::Nobody => false,
        ViewerListVisibility::ParticipantsOnly => {
            get_voice_connection_type(channel.id(), &user.id)
                .await?
                .as_deref()
                == Some("participant")
        }
        ViewerListVisibility::ModeratorsOnly => {
            let mut permissions = perms(db, &user).channel(&channel);
            let perm_value = calculate_channel_permissions(&mut permissions).await;
            perm_value.has(ChannelPermission::ManageChannel as u64)
        }
        ViewerListVisibility::Everyone => true,
    };

    let viewers = if can_see_list {
        let viewer_ids = get_viewer_list(channel.id()).await?;
        let mut users = Vec::new();
        for id in &viewer_ids {
            if let Ok(u) = Reference::from_unchecked(id).as_user(db).await {
                users.push(u.into(db, None).await);
            }
        }
        Some(users)
    } else {
        None
    };

    Ok(Json(v0::ViewersResponse { count, viewers }))
}
