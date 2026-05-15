use company_result::Result;

use crate::{Bot, FieldsBot, PartialBot};

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractBots: Sync + Send {
    /// Insert new bot into the database
    async fn insert_bot(&self, bot: &Bot) -> Result<()>;

    /// Fetch a bot by its id
    async fn fetch_bot(&self, id: &str) -> Result<Bot>;

    /// Fetch a bot by its token
    async fn fetch_bot_by_token(&self, token: &str) -> Result<Bot>;

    /// Fetch bots owned by a user
    async fn fetch_bots_by_user(&self, user_id: &str) -> Result<Vec<Bot>>;

    /// Get the number of bots owned by a user
    async fn get_number_of_bots_by_user(&self, user_id: &str) -> Result<usize>;

    /// Update bot with new information
    async fn update_bot(
        &self,
        id: &str,
        partial: &PartialBot,
        remove: Vec<FieldsBot>,
    ) -> Result<()>;

    /// Delete a bot from the database
    async fn delete_bot(&self, id: &str) -> Result<()>;

    /// Find AI Companion bot owned by a specific user (returns None if not found)
    async fn find_ai_companion_bot_by_owner(&self, owner_id: &str) -> Result<Option<Bot>>;

    /// Update which server an AI Companion bot has joined
    async fn update_bot_joined_server(
        &self,
        bot_id: &str,
        server_id: Option<String>,
    ) -> Result<()>;

    /// Update bot's public encryption key
    async fn update_bot_public_key(
        &self,
        bot_id: &str,
        public_key_pem: String,
        registered_at: iso8601_timestamp::Timestamp,
    ) -> Result<()>;
}
