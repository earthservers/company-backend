use company_database::{util::reference::Reference, BotCommand, Database, User};
use company_models::v0;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use validator::Validate;

/// # Create Bot Command
///
/// Register a new slash command for a bot.
#[openapi(tag = "Bots")]
#[post("/<bot>/commands", data = "<info>")]
pub async fn create_bot_command(
    db: &State<Database>,
    user: User,
    bot: Reference<'_>,
    info: Json<v0::DataCreateBotCommand>,
) -> Result<Json<v0::BotCommand>> {
    let info = info.into_inner();
    info.validate().map_err(|error| {
        create_error!(FailedValidation {
            error: error.to_string()
        })
    })?;

    let bot = bot.as_bot(db).await?;
    if bot.owner != user.id {
        return Err(create_error!(NotFound));
    }

    // Check command limit (max 50 commands per bot)
    let existing = db.fetch_bot_commands_by_bot(&bot.id).await?;
    if existing.len() >= 50 {
        return Err(create_error!(TooManyBotCommands));
    }

    // Check for duplicate command name
    if existing.iter().any(|cmd| cmd.name == info.name) {
        return Err(create_error!(DuplicateCommandName));
    }

    let command = BotCommand::create(
        db,
        bot.id,
        info.name,
        info.description,
        info.options.into_iter().map(Into::into).collect(),
    )
    .await?;

    Ok(Json(command.into()))
}
