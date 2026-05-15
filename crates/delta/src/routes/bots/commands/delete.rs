use company_database::{util::reference::Reference, Database, User};
use company_result::{create_error, Result};
use rocket::State;
use rocket_empty::EmptyResponse;

/// # Delete Bot Command
///
/// Delete a bot command by its id.
#[openapi(tag = "Bots")]
#[delete("/<bot>/commands/<command_id>")]
pub async fn delete_bot_command(
    db: &State<Database>,
    user: User,
    bot: Reference<'_>,
    command_id: &str,
) -> Result<EmptyResponse> {
    let bot = bot.as_bot(db).await?;
    if bot.owner != user.id {
        return Err(create_error!(NotFound));
    }

    let command = db.fetch_bot_command(command_id).await?;
    if command.bot_id != bot.id {
        return Err(create_error!(NotFound));
    }

    command.delete(db).await?;
    Ok(EmptyResponse)
}
