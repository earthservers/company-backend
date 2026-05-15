use bson::Document;
use company_result::Result;

use crate::{BotCommand, MongoDb};

use super::AbstractBotCommands;

static COL: &str = "bot_commands";

#[async_trait]
impl AbstractBotCommands for MongoDb {
    /// Insert a new bot command into the database
    async fn insert_bot_command(&self, command: &BotCommand) -> Result<()> {
        query!(self, insert_one, COL, &command).map(|_| ())
    }

    /// Fetch a bot command by its id
    async fn fetch_bot_command(&self, id: &str) -> Result<BotCommand> {
        query!(self, find_one_by_id, COL, id)?.ok_or_else(|| create_error!(NotFound))
    }

    /// Fetch all commands for a bot
    async fn fetch_bot_commands_by_bot(&self, bot_id: &str) -> Result<Vec<BotCommand>> {
        query!(
            self,
            find,
            COL,
            doc! {
                "bot_id": bot_id
            }
        )
    }

    /// Fetch commands for multiple bots at once
    async fn fetch_bot_commands_by_bots(&self, bot_ids: &[String]) -> Result<Vec<BotCommand>> {
        query!(
            self,
            find,
            COL,
            doc! {
                "bot_id": {
                    "$in": bot_ids
                }
            }
        )
    }

    /// Delete a bot command
    async fn delete_bot_command(&self, id: &str) -> Result<()> {
        query!(self, delete_one_by_id, COL, id).map(|_| ())
    }

    /// Delete all commands for a bot
    async fn delete_bot_commands_by_bot(&self, bot_id: &str) -> Result<()> {
        self.col::<Document>(COL)
            .delete_many(doc! {
                "bot_id": bot_id
            })
            .await
            .map_err(|_| create_database_error!("delete_many", COL))
            .map(|_| ())
    }
}
