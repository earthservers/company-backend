//! Send a DM notification from the "EarthCosmetics" bot user to a
//! cosmetic's creator when a moderation action is taken.
//!
//! Messages are persisted (`is_dm = false` on `send_without_notifications`)
//! so they bypass the ephemeral P2P broker path — this guarantees offline
//! recipients see them whenever they next fetch the DM channel's history,
//! independent of the 6-hour offline-queue TTL.
//!
//! Failure is non-fatal for the calling route: moderation actions succeed
//! even if the bot user isn't bootstrapped yet. Operators bootstrap it
//! once by signing up a regular Company account with username
//! "EarthCosmetics" and (manually, via MongoDB) flipping `privileged = true`
//! plus setting `is_official = true` if the schema has that field.

use company_database::{Channel, Database, Message};
use ulid::Ulid;

const BOT_USERNAME: &str = "EarthCosmetics";

pub struct ModerationNotificationInput<'a> {
    pub creator_user_id: &'a str,
    pub cosmetic_id: &'a str,
    pub cosmetic_display_name: &'a str,
    pub action: &'a str, // "approve" | "reject" | "request_changes"
    pub notes: Option<&'a str>,
}

/// Compose the notification text. Plain text so it renders on existing
/// clients without needing a new `SystemMessage` variant.
fn compose_text(input: &ModerationNotificationInput<'_>) -> String {
    let verb = match input.action {
        "approve" => "approved",
        "reject" => "rejected",
        "request_changes" => "had changes requested",
        _ => "moderated",
    };
    let head = format!(
        "**Your cosmetic \"{}\" has been {}.**",
        input.cosmetic_display_name, verb,
    );
    let notes = input.notes.unwrap_or("").trim();
    let body = if !notes.is_empty() {
        format!("\n\n_Notes from moderator:_\n> {}", notes.replace('\n', "\n> "))
    } else {
        String::new()
    };
    let footer = if input.action == "approve" {
        "\n\nYour cosmetic is now live in the catalog."
    } else if input.action == "reject" {
        "\n\nYou can revise and resubmit at any time \u{2014} resubmissions use a new id."
    } else {
        "\n\nMake the requested edits in Decoration Studio and resubmit to re-enter the queue."
    };
    format!("{head}{body}{footer}\n\n_— EarthCosmetics_\nCosmetic id: `{}`", input.cosmetic_id)
}

/// Send the notification. Returns Err only on truly unexpected failures —
/// "bot not found" and similar config-state errors are logged as warnings
/// and treated as success so the moderation action remains observable.
pub async fn send_moderation_notification(
    db: &Database,
    input: ModerationNotificationInput<'_>,
) -> Result<(), String> {
    // Look up the bot. If absent, this is a one-time setup issue —
    // log + skip notification, don't fail the calling action.
    //
    // The discriminator is "0000" for the default-registered first user.
    // If your install registered EarthCosmetics with a different
    // discriminator, fall through to the "0001" / "0002" / etc. lookups
    // here is intentionally not implemented — fix the discriminator
    // instead, or move to is_official-based lookup once that flag lands.
    let bot = match db.fetch_user_by_username(BOT_USERNAME, "0000").await {
        Ok(u) => u,
        Err(_) => {
            log::warn!(
                "EarthCosmetics bot user not found (looked up {BOT_USERNAME}#0000). \
                 Moderation notification skipped for cosmetic {}.",
                input.cosmetic_id,
            );
            return Ok(());
        }
    };

    let creator = match db.fetch_user(input.creator_user_id).await {
        Ok(u) => u,
        Err(_) => {
            log::warn!(
                "Creator user {} not found; moderation notification skipped for cosmetic {}.",
                input.creator_user_id,
                input.cosmetic_id,
            );
            return Ok(());
        }
    };

    // Create-or-fetch DM channel between bot and creator. Idempotent.
    let channel = Channel::create_dm(db, &bot, &creator)
        .await
        .map_err(|e| format!("create_dm failed: {e}"))?;

    let mut message = Message {
        id: Ulid::new().to_string(),
        nonce: None,
        channel: channel.id().to_string(),
        author: bot.id.clone(),
        webhook: None,
        content: Some(compose_text(&input)),
        system: None,
        attachments: None,
        edited: None,
        embeds: None,
        mentions: Some(vec![creator.id.clone()]),
        role_mentions: None,
        replies: None,
        reactions: Default::default(),
        interactions: Default::default(),
        masquerade: None,
        pinned: None,
        flags: None,
    };

    // is_dm: false forces DB persistence. Apparent paradox (a DirectMessage
    // channel with is_dm=false) is intentional — see this module's doc
    // comment. Online recipients still get the live event via pub/sub;
    // offline recipients fetch on reconnect via the channel's message list.
    message
        .send_without_notifications(db, None, None, /* is_dm */ false, /* embeds */ false, /* mentions_elsewhere */ false)
        .await
        .map_err(|e| format!("send_without_notifications failed: {e}"))?;

    Ok(())
}
