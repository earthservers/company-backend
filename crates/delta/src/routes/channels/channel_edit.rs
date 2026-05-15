use company_database::{
    util::{permissions::DatabasePermissionQuery, reference::Reference},
    voice::{delete_voice_channel, VoiceClient},
    Channel, Database, File, PartialChannel, SystemMessage, User, AMQP,
};
use company_models::v0;
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};
use rocket::{serde::json::Json, State};
use validator::Validate;

/// # Edit Channel
///
/// Edit a channel object by its id.
#[openapi(tag = "Channel Information")]
#[patch("/<target>", data = "<data>")]
pub async fn edit(
    db: &State<Database>,
    voice_client: &State<VoiceClient>,
    amqp: &State<AMQP>,
    user: User,
    target: Reference<'_>,
    data: Json<v0::DataEditChannel>,
) -> Result<Json<v0::Channel>> {
    let data = data.into_inner();
    data.validate().map_err(|error| {
        create_error!(FailedValidation {
            error: error.to_string()
        })
    })?;

    let mut channel = target.as_channel(db).await?;

    // For group channels, owner and moderators can edit.
    // For other channels, use the standard ManageChannel permission check.
    let is_group_owner_or_mod = if let Channel::Group {
        owner, moderators, ..
    } = &channel
    {
        user.id == *owner || moderators.contains(&user.id)
    } else {
        false
    };

    if !is_group_owner_or_mod {
        let mut query = DatabasePermissionQuery::new(db, &user).channel(&channel);
        calculate_channel_permissions(&mut query)
            .await
            .throw_if_lacking_channel_permission(ChannelPermission::ManageChannel)?;
    }

    // Moderator management fields are only for groups
    let has_mod_changes = data.add_moderators.is_some() || data.remove_moderators.is_some();

    if data.name.is_none()
        && data.description.is_none()
        && data.icon.is_none()
        && data.nsfw.is_none()
        && data.owner.is_none()
        && data.voice.is_none()
        && !has_mod_changes
        && data.remove.is_empty()
    {
        return Ok(Json(channel.into()));
    }

    let mut partial: PartialChannel = Default::default();

    // Transfer group ownership
    if let Some(new_owner) = data.owner {
        if let Channel::Group {
            owner, recipients, ..
        } = &mut channel
        {
            // Make sure we are the owner of this group
            if owner != &user.id {
                return Err(create_error!(NotOwner));
            }

            // Ensure user is part of group
            if !recipients.contains(&new_owner) {
                return Err(create_error!(NotInGroup));
            }

            // Transfer ownership
            partial.owner = Some(new_owner.to_string());
            let old_owner = std::mem::replace(owner, new_owner.to_string());

            // Notify clients
            SystemMessage::ChannelOwnershipChanged {
                from: old_owner,
                to: new_owner,
            }
        } else {
            return Err(create_error!(InvalidOperation));
        }
        .into_message(channel.id().to_string())
        .send(
            db,
            Some(amqp),
            user.as_author_for_system(),
            None,
            None,
            &channel,
            false,
        )
        .await
        .ok();
    }

    // Handle moderator management (owner only)
    if has_mod_changes {
        if let Channel::Group {
            owner,
            recipients,
            moderators,
            ..
        } = &mut channel
        {
            // Only the owner can manage moderators
            if owner != &user.id {
                return Err(create_error!(NotOwner));
            }

            if let Some(add_mods) = &data.add_moderators {
                for mod_id in add_mods {
                    // Must be a group recipient
                    if !recipients.contains(mod_id) {
                        return Err(create_error!(NotInGroup));
                    }
                    // Cannot make owner a moderator
                    if mod_id == owner {
                        continue;
                    }
                    // Don't add duplicates
                    if !moderators.contains(mod_id) {
                        moderators.push(mod_id.clone());
                    }
                }
            }

            if let Some(remove_mods) = &data.remove_moderators {
                moderators.retain(|m| !remove_mods.contains(m));
            }

            partial.moderators = Some(moderators.clone());
        } else {
            return Err(create_error!(InvalidOperation));
        }
    }

    match &mut channel {
        Channel::Group {
            id,
            name,
            description,
            icon,
            nsfw,
            ..
        } => {
            if data.remove.contains(&v0::FieldsChannel::Icon) {
                if let Some(icon) = &icon {
                    db.mark_attachment_as_deleted(&icon.id).await?;
                }
            }

            for field in &data.remove {
                match field {
                    v0::FieldsChannel::Description => {
                        description.take();
                    }
                    v0::FieldsChannel::Icon => {
                        icon.take();
                    }
                    _ => {}
                }
            }

            if let Some(icon_id) = data.icon {
                partial.icon = Some(File::use_channel_icon(db, &icon_id, id, &user.id).await?);
                *icon = partial.icon.clone();
            }

            if let Some(new_name) = data.name {
                *name = new_name.clone();
                partial.name = Some(new_name);
            }

            if let Some(new_description) = data.description {
                partial.description = Some(new_description);
                *description = partial.description.clone();
            }

            if let Some(new_nsfw) = data.nsfw {
                *nsfw = new_nsfw;
                partial.nsfw = Some(new_nsfw);
            }

            // Send out mutation system messages.
            if let Some(name) = &partial.name {
                SystemMessage::ChannelRenamed {
                    name: name.to_string(),
                    by: user.id.clone(),
                }
                .into_message(channel.id().to_string())
                .send(
                    db,
                    Some(amqp),
                    user.as_author_for_system(),
                    None,
                    None,
                    &channel,
                    false,
                )
                .await
                .ok();
            }

            if partial.description.is_some() {
                SystemMessage::ChannelDescriptionChanged {
                    by: user.id.clone(),
                }
                .into_message(channel.id().to_string())
                .send(
                    db,
                    Some(amqp),
                    user.as_author_for_system(),
                    None,
                    None,
                    &channel,
                    false,
                )
                .await
                .ok();
            }

            if partial.icon.is_some() {
                SystemMessage::ChannelIconChanged {
                    by: user.id.clone(),
                }
                .into_message(channel.id().to_string())
                .send(
                    db,
                    Some(amqp),
                    user.as_author_for_system(),
                    None,
                    None,
                    &channel,
                    false,
                )
                .await
                .ok();
            }
        }
        Channel::TextChannel {
            id,
            server: ref server_id,
            name,
            description,
            icon,
            nsfw,
            voice,
            ..
        } => {
            if data.remove.contains(&v0::FieldsChannel::Icon) {
                if let Some(icon) = &icon {
                    db.mark_attachment_as_deleted(&icon.id).await?;
                }
            }

            for field in &data.remove {
                match field {
                    v0::FieldsChannel::Description => {
                        description.take();
                    }
                    v0::FieldsChannel::Icon => {
                        icon.take();
                    }
                    v0::FieldsChannel::Voice => {
                        voice.take();
                    }
                    _ => {}
                }
            }

            if let Some(icon_id) = data.icon {
                partial.icon = Some(File::use_channel_icon(db, &icon_id, id, &user.id).await?);
                *icon = partial.icon.clone();
            }

            if let Some(new_name) = data.name {
                *name = new_name.clone();
                partial.name = Some(new_name);
            }

            if let Some(new_description) = data.description {
                partial.description = Some(new_description);
                *description = partial.description.clone();
            }

            if let Some(new_nsfw) = data.nsfw {
                *nsfw = new_nsfw;
                partial.nsfw = Some(new_nsfw);
            }

            if let Some(new_voice) = data.voice {
                // Validate bitrate against server owner's premium tier
                if let Some(new_bitrate) = new_voice.bitrate {
                    if new_bitrate < 32_000 {
                        return Err(create_error!(FailedValidation {
                            error: "Bitrate must be at least 32000 bps (32 kbps)".to_string()
                        }));
                    }

                    let server = db.fetch_server(server_id).await?;
                    let owner = db.fetch_user(&server.owner).await?;
                    let max_allowed = owner.max_voice_bitrate().await;

                    if new_bitrate > max_allowed {
                        return Err(create_error!(BitrateExceedsTier {
                            max: max_allowed,
                            message: format!(
                                "Bitrate {} bps exceeds your tier's maximum of {} bps. Upgrade your subscription for higher quality audio.",
                                new_bitrate, max_allowed
                            )
                        }));
                    }
                }

                *voice = Some(new_voice.clone().into());
                partial.voice = Some(new_voice.into());
            }
        }
        _ => return Err(create_error!(InvalidOperation)),
    };

    channel
        .update(
            db,
            partial,
            data.remove.into_iter().map(|f| f.into()).collect(),
        )
        .await?;

    if channel.voice().is_none() {
        delete_voice_channel(voice_client, channel.id(), channel.server()).await?;
    }

    Ok(Json(channel.into()))
}
