use company_database::{util::reference::Reference, Database, User};
use company_models::v0;
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;

/// # Fetch Channel Commands
///
/// Fetch all available slash commands in a channel from all bots present in the server.
#[openapi(tag = "Interactions")]
#[get("/<channel>/commands")]
pub async fn fetch_channel_commands(
    db: &State<Database>,
    _user: User,
    channel: Reference<'_>,
) -> Result<Json<Vec<v0::BotCommand>>> {
    let channel = channel.as_channel(db).await?;

    // Get the server this channel belongs to
    let server_id = match &channel {
        company_database::Channel::TextChannel { server, .. } => Some(server.clone()),
        _ => None,
    };

    // If the channel is in a server, find all bots in that server
    let bot_ids = if let Some(server_id) = server_id {
        let server = db.fetch_server(&server_id).await?;

        // Fetch all members, find which are bots
        let members = db.fetch_all_members(&server.id).await?;
        let mut bot_ids = Vec::new();
        for member in members {
            let user = db.fetch_user(&member.id.user).await?;
            if user.bot.is_some() {
                bot_ids.push(member.id.user);
            }
        }
        bot_ids
    } else {
        // For DMs/groups, check the participants
        match &channel {
            company_database::Channel::DirectMessage { recipients, .. }
            | company_database::Channel::Group { recipients, .. } => {
                let mut bot_ids = Vec::new();
                for user_id in recipients {
                    let user = db.fetch_user(user_id).await?;
                    if user.bot.is_some() {
                        bot_ids.push(user_id.clone());
                    }
                }
                bot_ids
            }
            _ => Vec::new(),
        }
    };

    if bot_ids.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let commands = db.fetch_bot_commands_by_bots(&bot_ids).await?;
    Ok(Json(commands.into_iter().map(Into::into).collect()))
}
