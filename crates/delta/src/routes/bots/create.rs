use company_database::{Bot, BotType, Database, PartialBot, User};
use company_models::v0;
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use validator::Validate;

use crate::util::license_verification::LicenseVerifier;

/// # Create Bot
///
/// Create a new Revolt bot.
///
/// For AI Companion bots, set `bot_type` to `"ai_companion"`.
/// This requires a valid AI Companion license and enforces a 1-bot-per-user limit.
#[openapi(tag = "Bots")]
#[post("/create", data = "<info>")]
pub async fn create_bot(
    db: &State<Database>,
    license_verifier: &State<LicenseVerifier>,
    user: User,
    info: Json<v0::DataCreateBot>,
) -> Result<Json<v0::BotWithUserResponse>> {
    let info = info.into_inner();
    info.validate().map_err(|error| {
        create_error!(FailedValidation {
            error: error.to_string()
        })
    })?;

    let is_ai_companion = info
        .bot_type
        .as_ref()
        .map(|t| *t == v0::BotType::AiCompanion)
        .unwrap_or(false);

    if is_ai_companion {
        // Verify license with license server before creating
        let license = license_verifier
            .verify_license(&user.id)
            .await
            .map_err(|msg| {
                create_error!(LicenseVerificationFailed {
                    message: format!(
                        "{}. Visit https://companion.earthservers.net to get a license.",
                        msg
                    )
                })
            })?;

        // Create bot with AI Companion metadata
        let partial = PartialBot {
            bot_type: Some(BotType::AiCompanion),
            license_type: Some(license.license_type),
            license_verified_at: Some(Timestamp::now_utc()),
            ..Default::default()
        };

        let (bot, user) = Bot::create(db, info.name, &user, partial).await?;
        Ok(Json(v0::BotWithUserResponse {
            bot: bot.into(),
            user: user.into_self(false).await,
        }))
    } else {
        // Regular bot creation (unchanged)
        let (bot, user) = Bot::create(db, info.name, &user, None).await?;
        Ok(Json(v0::BotWithUserResponse {
            bot: bot.into(),
            user: user.into_self(false).await,
        }))
    }
}

#[cfg(test)]
mod test {
    use crate::{rocket, util::test::TestHarness};
    use company_models::v0;
    use rocket::http::{ContentType, Header, Status};

    #[rocket::async_test]
    async fn create_bot() {
        let harness = TestHarness::new().await;
        let (_, session, _) = harness.new_user().await;

        let response = harness
            .client
            .post("/bots/create")
            .header(Header::new("x-session-token", session.token.to_string()))
            .header(ContentType::JSON)
            .body(
                json!(v0::DataCreateBot {
                    name: TestHarness::rand_string(),
                    bot_type: None,
                })
                .to_string(),
            )
            .dispatch()
            .await;

        assert_eq!(response.status(), Status::Ok);

        let bot: v0::Bot = response.into_json().await.expect("`Bot`");
        assert!(harness.db.fetch_bot(&bot.id).await.is_ok());
    }
}
