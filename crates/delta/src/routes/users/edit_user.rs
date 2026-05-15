use company_database::FieldsUser;
use company_database::{util::reference::Reference, Database, File, PartialUser, User};
use company_models::v0;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use validator::Validate;

use crate::util::user_events::{publish_user_update, UserEventBroadcaster};

/// # Edit User
///
/// Edit currently authenticated user.
#[openapi(tag = "User Information")]
#[patch("/<target>", data = "<data>")]
pub async fn edit(
    db: &State<Database>,
    user_events: &State<UserEventBroadcaster>,
    mut user: User,
    target: Reference<'_>,
    data: Json<v0::DataEditUser>,
) -> Result<Json<v0::User>> {
    let data = data.into_inner();
    data.validate().map_err(|error| {
        create_error!(FailedValidation {
            error: error.to_string()
        })
    })?;

    // Filter out invalid edit fields
    if !user.privileged && (data.badges.is_some() || data.flags.is_some()) {
        return Err(create_error!(NotPrivileged));
    }

    // If we want to edit a different user than self, ensure we have
    // permissions and subsequently replace the user in question
    if target.id != "@me" && target.id != user.id {
        let target_user = target.as_user(db).await?;
        let is_bot_owner = target_user
            .bot
            .as_ref()
            .map(|bot| bot.owner == user.id)
            .unwrap_or_default();

        if !is_bot_owner && !user.privileged {
            return Err(create_error!(NotPrivileged));
        }

        user = target_user;
    }

    // Exit out early if nothing is changed
    if data.display_name.is_none()
        && data.status.is_none()
        && data.profile.is_none()
        && data.avatar.is_none()
        && data.badges.is_none()
        && data.flags.is_none()
        && data.remove.is_empty()
    {
        return Ok(Json(user.into_self(false).await));
    }

    // 1. Remove fields from object
    if data.remove.contains(&v0::FieldsUser::Avatar) {
        if let Some(avatar) = &user.avatar {
            db.mark_attachment_as_deleted(&avatar.id).await?;
        }
    }

    if data.remove.contains(&v0::FieldsUser::ProfileBackground) {
        if let Some(profile) = &user.profile {
            if let Some(background) = &profile.background {
                db.mark_attachment_as_deleted(&background.id).await?;
            }
        }
    }

    for field in &data.remove {
        let field: FieldsUser = field.clone().into();
        user.remove_field(&field);
    }

    let mut partial: PartialUser = PartialUser {
        display_name: data.display_name,
        badges: data.badges,
        flags: data.flags,
        presence_style: data.presence_style,
        ..Default::default()
    };

    // 2. Apply new avatar
    if let Some(avatar) = data.avatar {
        // Try the attachments collection first (legacy Autumn)
        match File::use_user_avatar(db, &avatar, &user.id, &user.id).await {
            Ok(file) => partial.avatar = Some(file),
            Err(_) => {
                // Asset was uploaded via /assets endpoint — construct a minimal File record
                partial.avatar = Some(File {
                    id: avatar.clone(),
                    tag: "avatars".to_string(),
                    filename: avatar,
                    hash: None,
                    uploaded_at: None,
                    uploader_id: Some(user.id.clone()),
                    used_for: None,
                    deleted: None,
                    reported: None,
                    metadata: company_database::Metadata::File,
                    content_type: "image/png".to_string(),
                    size: 0,
                    message_id: None,
                    user_id: Some(user.id.clone()),
                    server_id: None,
                    object_id: None,
                });
            }
        }
    }

    // 3. Apply new status
    if let Some(status) = data.status {
        let mut new_status = user.status.take().unwrap_or_default();
        if let Some(text) = status.text {
            new_status.text = Some(text);
        }

        if let Some(presence) = status.presence {
            new_status.presence = Some(presence.into());
        }

        partial.status = Some(new_status);
    }

    // 4. Apply new profile
    if let Some(profile) = data.profile {
        let mut new_profile = user.profile.take().unwrap_or_default();
        if let Some(content) = profile.content {
            new_profile.content = Some(content);
        }

        if let Some(background) = profile.background {
            match File::use_background(db, &background, &user.id, &user.id).await {
                Ok(file) => new_profile.background = Some(file),
                Err(_) => {
                    // Asset uploaded via /assets endpoint — construct File directly
                    new_profile.background = Some(File {
                        id: background.clone(),
                        tag: "backgrounds".to_string(),
                        filename: background,
                        hash: None,
                        uploaded_at: None,
                        uploader_id: Some(user.id.clone()),
                        used_for: None,
                        deleted: None,
                        reported: None,
                        metadata: company_database::Metadata::File,
                        content_type: "image/png".to_string(),
                        size: 0,
                        message_id: None,
                        user_id: Some(user.id.clone()),
                        server_id: None,
                        object_id: None,
                    });
                }
            }
        }

        partial.profile = Some(new_profile);
    }

    user.update(
        db,
        partial,
        data.remove.into_iter().map(Into::into).collect(),
    )
    .await?;

    publish_user_update(user_events, &user.id);
    Ok(Json(user.into_self(false).await))
}
