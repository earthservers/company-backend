use company_database::util::permissions::DatabasePermissionQuery;
use company_database::{util::reference::Reference, BotType, Database, EncryptedChannelInvitation, User};
use company_database::{Member, AMQP};
use company_models::v0;
use company_permissions::{
    calculate_channel_permissions, calculate_server_permissions, ChannelPermission,
};
use company_result::{create_error, Result};
use base64::Engine;
use iso8601_timestamp::Timestamp;
use rocket::State;

use rocket::serde::json::Json;
use rocket_empty::EmptyResponse;

use crate::util::encryption::{encrypt_for_public_key, validate_public_key};

/// # Invite Bot
///
/// Invite a bot to a server or group by its id.
///
/// AI Companion bots are restricted to exactly 1 server at a time.
/// They must leave their current server before joining another.
#[openapi(tag = "Bots")]
#[post("/<target>/invite", data = "<dest>")]
pub async fn invite_bot(
    db: &State<Database>,
    amqp: &State<AMQP>,
    user: User,
    target: Reference<'_>,
    dest: Json<v0::InviteBotDestination>,
) -> Result<EmptyResponse> {
    if user.bot.is_some() {
        return Err(create_error!(IsBot));
    }

    let bot = target.as_bot(db).await?;
    if !bot.public && bot.owner != user.id {
        return Err(create_error!(BotIsPrivate));
    }

    let bot_user = db.fetch_user(&bot.id).await?;

    match dest.into_inner() {
        v0::InviteBotDestination::Server { server } => {
            let server = db.fetch_server(&server).await?;

            // Enforce AI Companion 1-server limit
            if bot.bot_type == BotType::AiCompanion {
                if let Some(ref current_server_id) = bot.joined_server_id {
                    if current_server_id == &server.id {
                        // Already in this server - idempotent
                        return Ok(EmptyResponse);
                    }

                    // Self-healing: verify the membership still exists
                    if db.fetch_member(current_server_id, &bot.id).await.is_ok() {
                        return Err(create_error!(AICompanionServerLimit {
                            message: format!(
                                "AI Companion bot '{}' is already in a server. \
                                 It can only be in 1 server at a time. \
                                 Remove it from the current server first.",
                                bot.id
                            )
                        }));
                    }

                    // Stale joined_server_id - clear it and allow joining
                    let _ = db.update_bot_joined_server(&bot.id, None).await;
                }
            }

            let mut query = DatabasePermissionQuery::new(db, &user).server(&server);
            calculate_server_permissions(&mut query)
                .await
                .throw_if_lacking_channel_permission(ChannelPermission::ManageServer)?;

            let result = Member::create(db, &server, &bot_user, None).await;

            // Track which server the AI Companion bot joined
            if result.is_ok() && bot.bot_type == BotType::AiCompanion {
                let _ = db
                    .update_bot_joined_server(&bot.id, Some(server.id.clone()))
                    .await;
            }

            // After bot joins server, create encrypted channel invitations if bot has a public key
            if result.is_ok() {
                if let Some(ref public_key_pem) = bot.public_key_pem {
                    if let Ok(public_key) = validate_public_key(public_key_pem) {
                        if let Ok(server_channels) = db.fetch_channels(&server.channels).await {
                            for channel in server_channels {
                                if channel.is_encrypted() {
                                    if let Ok(key_b64) =
                                        db.get_channel_encryption_key(channel.id()).await
                                    {
                                        if let Ok(key_bytes) =
                                            base64::engine::general_purpose::STANDARD
                                                .decode(&key_b64)
                                        {
                                            if let Ok(encrypted_key) =
                                                encrypt_for_public_key(&key_bytes, &public_key)
                                            {
                                                let now = Timestamp::now_utc();
                                                let now_ms = now.duration_since(Timestamp::UNIX_EPOCH).whole_milliseconds() as i64;
                                                let expires_at = Timestamp::from_unix_timestamp_ms(
                                                    now_ms + 24 * 60 * 60 * 1000,
                                                );

                                                let invitation = EncryptedChannelInvitation {
                                                    id: ulid::Ulid::new().to_string(),
                                                    channel_id: channel.id().to_string(),
                                                    bot_id: bot.id.clone(),
                                                    inviter_id: user.id.clone(),
                                                    encrypted_channel_key: hex::encode(
                                                        encrypted_key,
                                                    ),
                                                    created_at: now,
                                                    expires_at,
                                                };

                                                let _ = db
                                                    .insert_encrypted_invitation(&invitation)
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            result.map(|_| EmptyResponse)
        }
        v0::InviteBotDestination::Group { group } => {
            // AI Companion bots cannot join groups - they are server-only
            if bot.bot_type == BotType::AiCompanion {
                return Err(create_error!(AICompanionServerLimit {
                    message: "AI Companion bots can only join servers, not groups.".to_string()
                }));
            }

            let mut channel = db.fetch_channel(&group).await?;

            let mut query = DatabasePermissionQuery::new(db, &user).channel(&channel);
            calculate_channel_permissions(&mut query)
                .await
                .throw_if_lacking_channel_permission(ChannelPermission::InviteOthers)?;

            let result = channel
                .add_user_to_group(db, amqp, &bot_user, &user.id)
                .await;

            // Create encrypted channel invitation for group if bot has a public key
            if result.is_ok() && channel.is_encrypted() {
                if let Some(ref public_key_pem) = bot.public_key_pem {
                    if let Ok(public_key) = validate_public_key(public_key_pem) {
                        if let Ok(key_b64) = db.get_channel_encryption_key(channel.id()).await {
                            if let Ok(key_bytes) =
                                base64::engine::general_purpose::STANDARD.decode(&key_b64)
                            {
                                if let Ok(encrypted_key) =
                                    encrypt_for_public_key(&key_bytes, &public_key)
                                {
                                    let now = Timestamp::now_utc();
                                    let now_ms = now.duration_since(Timestamp::UNIX_EPOCH).whole_milliseconds() as i64;
                                    let expires_at = Timestamp::from_unix_timestamp_ms(
                                        now_ms + 24 * 60 * 60 * 1000,
                                    );

                                    let invitation = EncryptedChannelInvitation {
                                        id: ulid::Ulid::new().to_string(),
                                        channel_id: channel.id().to_string(),
                                        bot_id: bot.id.clone(),
                                        inviter_id: user.id.clone(),
                                        encrypted_channel_key: hex::encode(encrypted_key),
                                        created_at: now,
                                        expires_at,
                                    };

                                    let _ = db.insert_encrypted_invitation(&invitation).await;
                                }
                            }
                        }
                    }
                }
            }

            result.map(|_| EmptyResponse)
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{rocket, util::test::TestHarness};
    use company_database::{events::client::EventV1, Bot, Channel, Server};
    use company_models::v0::{self, DataCreateServer};
    use rocket::http::{ContentType, Header, Status};

    #[rocket::async_test]
    async fn invite_bot_to_group() {
        let mut harness = TestHarness::new().await;
        let (_, session, user) = harness.new_user().await;

        let (bot, _) = Bot::create(&harness.db, TestHarness::rand_string(), &user, None)
            .await
            .expect("`Bot`");

        let group = Channel::create_group(
            &harness.db,
            v0::DataCreateGroup {
                name: TestHarness::rand_string(),
                ..Default::default()
            },
            user.id.to_string(),
        )
        .await
        .unwrap();

        let response = harness
            .client
            .post(format!("/bots/{}/invite", bot.id))
            .header(ContentType::JSON)
            .body(
                json!(v0::InviteBotDestination::Group {
                    group: group.id().to_string()
                })
                .to_string(),
            )
            .header(Header::new("x-session-token", session.token.to_string()))
            .dispatch()
            .await;

        assert_eq!(response.status(), Status::NoContent);
        drop(response);

        let event = harness
            .wait_for_event(group.id(), |event| match event {
                EventV1::ChannelGroupJoin { id, .. } => id == group.id(),
                _ => false,
            })
            .await;

        match event {
            EventV1::ChannelGroupJoin { user, .. } => {
                assert_eq!(bot.id, user);
            }
            _ => unreachable!(),
        }
    }

    #[rocket::async_test]
    async fn invite_bot_to_server() {
        let mut harness = TestHarness::new().await;
        let (_, session, user) = harness.new_user().await;

        let (bot, _) = Bot::create(&harness.db, TestHarness::rand_string(), &user, None)
            .await
            .expect("`Bot`");

        let (server, _) = Server::create(
            &harness.db,
            DataCreateServer {
                name: TestHarness::rand_string(),
                ..Default::default()
            },
            &user,
            false,
        )
        .await
        .unwrap();

        let response = harness
            .client
            .post(format!("/bots/{}/invite", bot.id))
            .header(ContentType::JSON)
            .body(
                json!(v0::InviteBotDestination::Server {
                    server: server.id.to_string()
                })
                .to_string(),
            )
            .header(Header::new("x-session-token", session.token.to_string()))
            .dispatch()
            .await;

        assert_eq!(response.status(), Status::NoContent);
        drop(response);

        let event = harness
            .wait_for_event(&server.id, |event| match event {
                EventV1::ServerMemberJoin { id, .. } => id == &server.id,
                _ => false,
            })
            .await;

        match event {
            EventV1::ServerMemberJoin { member, .. } => {
                assert_eq!(bot.id, member.id.user);
            }
            _ => unreachable!(),
        }
    }
}
