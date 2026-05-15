use company_database::util::permissions::DatabasePermissionQuery;
use company_database::{util::reference::Reference, Database, User};
use company_database::events::client::EventV1;
use company_models::v0;
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use ulid::Ulid;
use validator::Validate;

use crate::util::interaction_store::{InteractionStore, PendingInteraction};

/// # Invoke Slash Command
///
/// Invoke a bot slash command in a channel. Creates an interaction and dispatches it to the bot.
#[openapi(tag = "Interactions")]
#[post("/<channel>/interactions", data = "<data>")]
pub async fn invoke_command(
    db: &State<Database>,
    store: &State<InteractionStore>,
    user: User,
    channel: Reference<'_>,
    data: Json<v0::DataInvokeCommand>,
) -> Result<Json<v0::InvokeCommandResponse>> {
    let data = data.into_inner();

    // Validate the request data
    data.validate().map_err(|error| {
        create_error!(FailedValidation {
            error: error.to_string()
        })
    })?;

    let channel = channel.as_channel(db).await?;

    // Ensure the user has permission to send messages in this channel
    let mut query = DatabasePermissionQuery::new(db, &user).channel(&channel);
    let permissions = calculate_channel_permissions(&mut query).await;
    permissions.throw_if_lacking_channel_permission(ChannelPermission::SendMessage)?;

    // Fetch the command to validate it exists and get the bot_id
    let command = db.fetch_bot_command(&data.command_id).await?;

    // Verify the bot is accessible from this channel
    let bot_accessible = match &channel {
        company_database::Channel::TextChannel { server, .. } => {
            db.fetch_member(server, &command.bot_id).await.is_ok()
        }
        company_database::Channel::DirectMessage { recipients, .. }
        | company_database::Channel::Group { recipients, .. } => {
            recipients.contains(&command.bot_id)
        }
        _ => false,
    };

    if !bot_accessible {
        return Err(create_error!(NotFound));
    }

    // Generate interaction ID and token
    let interaction_id = Ulid::new().to_string();
    let token = Ulid::new().to_string();

    let server_id = channel.server().map(|s| s.to_string());
    let channel_id = channel.id().to_string();

    // Build the Interaction event for the bot
    let interaction = v0::Interaction {
        id: interaction_id.clone(),
        interaction_type: v0::InteractionType::Command,
        command: command.name.clone(),
        options: data.options,
        channel_id: channel_id.clone(),
        user_id: user.id.clone(),
        server_id,
        bot_id: command.bot_id.clone(),
        token: token.clone(),
    };

    // Store pending interaction for callback validation
    store.insert(
        interaction_id.clone(),
        PendingInteraction {
            channel_id,
            user_id: user.id,
            bot_id: command.bot_id.clone(),
            command_name: command.name,
            token,
            created_at: std::time::Instant::now(),
        },
    );

    // Clean up expired interactions periodically
    store.cleanup_expired();

    // Dispatch InteractionCreate to the bot via its private channel
    EventV1::InteractionCreate(interaction)
        .private(command.bot_id)
        .await;

    Ok(Json(v0::InvokeCommandResponse {
        id: interaction_id,
    }))
}
