use iso8601_timestamp::Timestamp;
use company_result::Result;

use crate::{Bot, FieldsBot, PartialBot};
use crate::{IntoDocumentPath, MongoDb};

use super::AbstractBots;

static COL: &str = "bots";

#[async_trait]
impl AbstractBots for MongoDb {
    /// Insert new bot into the database
    async fn insert_bot(&self, bot: &Bot) -> Result<()> {
        query!(self, insert_one, COL, &bot).map(|_| ())
    }

    /// Fetch a bot by its id
    async fn fetch_bot(&self, id: &str) -> Result<Bot> {
        query!(self, find_one_by_id, COL, id)?.ok_or_else(|| create_error!(NotFound))
    }

    /// Fetch a bot by its token
    async fn fetch_bot_by_token(&self, token: &str) -> Result<Bot> {
        query!(
            self,
            find_one,
            COL,
            doc! {
                "token": token
            }
        )?
        .ok_or_else(|| create_error!(NotFound))
    }

    /// Fetch bots owned by a user
    async fn fetch_bots_by_user(&self, user_id: &str) -> Result<Vec<Bot>> {
        query!(
            self,
            find,
            COL,
            doc! {
                "owner": user_id
            }
        )
    }

    /// Get the number of bots owned by a user
    async fn get_number_of_bots_by_user(&self, user_id: &str) -> Result<usize> {
        query!(
            self,
            count_documents,
            COL,
            doc! {
                "owner": user_id
            }
        )
        .map(|v| v as usize)
    }

    /// Update bot with new information
    async fn update_bot(
        &self,
        id: &str,
        partial: &PartialBot,
        remove: Vec<FieldsBot>,
    ) -> Result<()> {
        query!(
            self,
            update_one_by_id,
            COL,
            id,
            partial,
            remove.iter().map(|x| x as &dyn IntoDocumentPath).collect(),
            None
        )
        .map(|_| ())
    }

    /// Delete a bot from the database
    async fn delete_bot(&self, id: &str) -> Result<()> {
        query!(self, delete_one_by_id, COL, id).map(|_| ())
    }

    /// Find AI Companion bot owned by a specific user
    async fn find_ai_companion_bot_by_owner(&self, owner_id: &str) -> Result<Option<Bot>> {
        query!(
            self,
            find_one,
            COL,
            doc! {
                "owner": owner_id,
                "bot_type": "ai_companion"
            }
        )
    }

    /// Update which server an AI Companion bot has joined
    async fn update_bot_joined_server(
        &self,
        bot_id: &str,
        server_id: Option<String>,
    ) -> Result<()> {
        let partial = PartialBot {
            joined_server_id: server_id,
            ..Default::default()
        };

        query!(
            self,
            update_one_by_id,
            COL,
            bot_id,
            &partial,
            Vec::<&dyn IntoDocumentPath>::new(),
            None
        )
        .map(|_| ())
    }

    /// Update bot's public encryption key
    async fn update_bot_public_key(
        &self,
        bot_id: &str,
        public_key_pem: String,
        registered_at: Timestamp,
    ) -> Result<()> {
        let partial = PartialBot {
            public_key_pem: Some(public_key_pem),
            public_key_registered_at: Some(registered_at),
            ..Default::default()
        };

        query!(
            self,
            update_one_by_id,
            COL,
            bot_id,
            &partial,
            Vec::<&dyn IntoDocumentPath>::new(),
            None
        )
        .map(|_| ())
    }
}

impl IntoDocumentPath for FieldsBot {
    fn as_path(&self) -> Option<&'static str> {
        match self {
            FieldsBot::InteractionsURL => Some("interactions_url"),
            FieldsBot::Token => None,
        }
    }
}
