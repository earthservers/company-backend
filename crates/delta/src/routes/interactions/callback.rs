use company_database::util::idempotency::IdempotencyKey;
use company_database::{Database, Message, User, AMQP};
use company_models::v0;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use rocket_empty::EmptyResponse;

use crate::util::interaction_store::InteractionStore;

/// # Interaction Callback
///
/// Respond to an interaction. Used by bots to reply to slash command invocations.
/// The bot must authenticate using its bot token.
#[openapi(tag = "Interactions")]
#[post("/<interaction_id>/callback", data = "<data>")]
pub async fn interaction_callback(
    db: &State<Database>,
    amqp: &State<AMQP>,
    store: &State<InteractionStore>,
    user: User,
    interaction_id: &str,
    data: Json<v0::DataInteractionResponse>,
) -> Result<EmptyResponse> {
    // Only bots can respond to interactions
    if user.bot.is_none() {
        return Err(create_error!(IsNotBot));
    }

    let data = data.into_inner();

    // Look up the pending interaction
    let pending = store.get(interaction_id).ok_or_else(|| create_error!(UnknownInteraction))?;

    // Verify this bot is the correct handler
    if pending.bot_id != user.id {
        return Err(create_error!(UnknownInteraction));
    }

    match data.response_type {
        v0::InteractionResponseType::Reply => {
            let response_data = data.data.unwrap_or_default();
            let content = response_data.content.unwrap_or_default();

            if content.is_empty() {
                return Err(create_error!(EmptyMessage));
            }

            // Consume the interaction (one-time use for Reply)
            store.take(interaction_id);

            // Fetch the channel to create the message in
            let channel = db.fetch_channel(&pending.channel_id).await?;

            // Build the message data
            let message_data = v0::DataMessageSend {
                content: Some(content),
                nonce: None,
                attachments: None,
                replies: None,
                embeds: None,
                masquerade: None,
                interactions: None,
                flags: None,
            };

            // Build the bot's user model for the message author
            let bot_user: v0::User = user.clone().into(db, None).await;
            let bot_user_clone = bot_user.clone();

            // Create the message as the bot
            Message::create_from_api(
                db,
                Some(amqp),
                channel,
                message_data,
                v0::MessageAuthor::User(&bot_user),
                Some(bot_user_clone),
                None, // Bots don't have member objects for this context
                user.limits().await,
                IdempotencyKey::unchecked_from_string(interaction_id.to_string()),
                true,  // generate_embeds
                false, // allow_mentions (bot responses don't @ people by default)
            )
            .await?;

            Ok(EmptyResponse)
        }
        v0::InteractionResponseType::DeferredReply => {
            // Acknowledge - the interaction stays in the store
            // The bot will follow up later with an edit or another callback
            Ok(EmptyResponse)
        }
        v0::InteractionResponseType::Autocomplete => {
            // Autocomplete responses don't create messages
            // The choices are sent back to the invoking user
            // For now this is a no-op; the frontend will handle autocomplete
            // via a separate polling mechanism in Phase 3
            Ok(EmptyResponse)
        }
    }
}
