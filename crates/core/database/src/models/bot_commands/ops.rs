use company_result::Result;

use crate::BotCommand;

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractBotCommands: Sync + Send {
    /// Insert a new bot command into the database
    async fn insert_bot_command(&self, command: &BotCommand) -> Result<()>;

    /// Fetch a bot command by its id
    async fn fetch_bot_command(&self, id: &str) -> Result<BotCommand>;

    /// Fetch all commands for a bot
    async fn fetch_bot_commands_by_bot(&self, bot_id: &str) -> Result<Vec<BotCommand>>;

    /// Fetch commands for multiple bots at once
    async fn fetch_bot_commands_by_bots(&self, bot_ids: &[String]) -> Result<Vec<BotCommand>>;

    /// Delete a bot command
    async fn delete_bot_command(&self, id: &str) -> Result<()>;

    /// Delete all commands for a bot
    async fn delete_bot_commands_by_bot(&self, bot_id: &str) -> Result<()>;
}
