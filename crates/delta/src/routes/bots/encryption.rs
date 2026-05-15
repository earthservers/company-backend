use company_database::{util::reference::Reference, Database, User};
use company_models::v0;
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;

use crate::util::encryption::validate_public_key;

/// # Register Bot Public Key
///
/// Register an RSA public key for a bot to enable E2E encrypted channel participation.
/// Must be called by the bot itself (authenticated via x-bot-token header).
/// The key must be a valid PEM-encoded RSA public key with a minimum of 2048 bits.
#[openapi(tag = "Bots")]
#[post("/encryption/register-public-key", data = "<data>")]
pub async fn register_public_key(
    db: &State<Database>,
    user: User,
    data: Json<v0::DataRegisterPublicKey>,
) -> Result<Json<()>> {
    // Must be called by a bot (authenticated via x-bot-token)
    let _bot_info = user.bot.as_ref().ok_or_else(|| create_error!(IsNotBot))?;

    // Validate PEM format and minimum key size
    let _ = validate_public_key(&data.public_key)?;

    // Update bot's public key
    db.update_bot_public_key(&user.id, data.into_inner().public_key, Timestamp::now_utc())
        .await?;

    Ok(Json(()))
}

/// # Accept Encrypted Channel Invite
///
/// Accept an encrypted channel invitation and retrieve the encrypted channel key.
/// Must be called by the bot itself (authenticated via x-bot-token header).
/// The returned encrypted_channel_key is hex-encoded and encrypted with the bot's RSA public key.
/// The invitation is consumed (deleted) after acceptance.
#[openapi(tag = "Bots")]
#[post("/encryption/<bot_id>/accept-invite/<channel_id>")]
pub async fn accept_encrypted_invite(
    db: &State<Database>,
    user: User,
    bot_id: Reference<'_>,
    channel_id: Reference<'_>,
) -> Result<Json<v0::EncryptedChannelKeyResponse>> {
    // Must be called by a bot
    let _bot_info = user.bot.as_ref().ok_or_else(|| create_error!(IsNotBot))?;

    // Bot can only accept its own invitations
    if user.id != bot_id.id {
        return Err(create_error!(NotFound));
    }

    // Fetch the pending invitation
    let invitation = db
        .fetch_encrypted_invitation(channel_id.id, bot_id.id)
        .await?
        .ok_or_else(|| create_error!(NotFound))?;

    // Check expiry
    if invitation.expires_at < Timestamp::now_utc() {
        // Delete the expired invitation
        let _ = db.delete_encrypted_invitation(&invitation.id).await;
        return Err(create_error!(InvitationExpired));
    }

    let response = v0::EncryptedChannelKeyResponse {
        channel_id: invitation.channel_id.clone(),
        encrypted_channel_key: invitation.encrypted_channel_key.clone(),
    };

    // Delete the consumed invitation
    db.delete_encrypted_invitation(&invitation.id).await?;

    Ok(Json(response))
}
