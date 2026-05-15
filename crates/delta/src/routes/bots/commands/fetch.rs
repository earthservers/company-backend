use company_database::{util::reference::Reference, Database, User};
use company_models::v0;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;

/// # Fetch Bot Commands
///
/// Fetch all commands registered for a bot.
#[openapi(tag = "Bots")]
#[get("/<bot>/commands")]
pub async fn fetch_bot_commands(
    db: &State<Database>,
    user: User,
    bot: Reference<'_>,
) -> Result<Json<Vec<v0::BotCommand>>> {
    let bot = bot.as_bot(db).await?;
    if bot.owner != user.id {
        return Err(create_error!(NotFound));
    }

    let commands = db.fetch_bot_commands_by_bot(&bot.id).await?;
    Ok(Json(commands.into_iter().map(Into::into).collect()))
}
