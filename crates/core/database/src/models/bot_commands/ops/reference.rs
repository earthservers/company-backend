use company_result::Result;

use crate::{BotCommand, ReferenceDb};

use super::AbstractBotCommands;

#[async_trait]
impl AbstractBotCommands for ReferenceDb {
    /// Insert a new bot command into the database
    async fn insert_bot_command(&self, command: &BotCommand) -> Result<()> {
        let mut commands = self.bot_commands.lock().await;
        if commands.contains_key(&command.id) {
            Err(create_database_error!("insert", "bot_commands"))
        } else {
            commands.insert(command.id.to_string(), command.clone());
            Ok(())
        }
    }

    /// Fetch a bot command by its id
    async fn fetch_bot_command(&self, id: &str) -> Result<BotCommand> {
        let commands = self.bot_commands.lock().await;
        commands
            .get(id)
            .cloned()
            .ok_or_else(|| create_error!(NotFound))
    }

    /// Fetch all commands for a bot
    async fn fetch_bot_commands_by_bot(&self, bot_id: &str) -> Result<Vec<BotCommand>> {
        let commands = self.bot_commands.lock().await;
        Ok(commands
            .values()
            .filter(|cmd| cmd.bot_id == bot_id)
            .cloned()
            .collect())
    }

    /// Fetch commands for multiple bots at once
    async fn fetch_bot_commands_by_bots(&self, bot_ids: &[String]) -> Result<Vec<BotCommand>> {
        let commands = self.bot_commands.lock().await;
        Ok(commands
            .values()
            .filter(|cmd| bot_ids.contains(&cmd.bot_id))
            .cloned()
            .collect())
    }

    /// Delete a bot command
    async fn delete_bot_command(&self, id: &str) -> Result<()> {
        let mut commands = self.bot_commands.lock().await;
        if commands.remove(id).is_some() {
            Ok(())
        } else {
            Err(create_error!(NotFound))
        }
    }

    /// Delete all commands for a bot
    async fn delete_bot_commands_by_bot(&self, bot_id: &str) -> Result<()> {
        let mut commands = self.bot_commands.lock().await;
        commands.retain(|_, cmd| cmd.bot_id != bot_id);
        Ok(())
    }
}
