use company_result::Result;
use ulid::Ulid;

use crate::Database;

auto_derived!(
    /// Type of a command option
    pub enum CommandOptionType {
        String,
        Integer,
        Boolean,
        User,
        Channel,
        Role,
        Number,
    }

    /// A choice for a command option
    pub struct CommandChoice {
        /// Display name for the choice
        pub name: String,
        /// Value sent to the bot when this choice is selected
        pub value: String,
    }

    /// A command option (argument)
    pub struct CommandOption {
        /// Option name
        pub name: String,
        /// Option description
        pub description: String,
        /// Option type
        #[serde(rename = "type")]
        pub option_type: CommandOptionType,
        /// Whether this option is required
        #[serde(default)]
        pub required: bool,
        /// Pre-defined choices for this option
        #[serde(skip_serializing_if = "Option::is_none")]
        pub choices: Option<Vec<CommandChoice>>,
        /// Whether this option supports autocomplete
        #[serde(skip_serializing_if = "crate::if_false", default)]
        pub autocomplete: bool,
    }

    /// A bot slash command
    pub struct BotCommand {
        /// Command Id
        #[serde(rename = "_id")]
        pub id: String,
        /// Bot Id (the bot user's id)
        pub bot_id: String,
        /// Command name (lowercase, no spaces, 1-32 chars)
        pub name: String,
        /// Command description
        pub description: String,
        /// Command options (arguments)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub options: Vec<CommandOption>,
    }
);

#[allow(clippy::derivable_impls)]
impl Default for BotCommand {
    fn default() -> Self {
        Self {
            id: Default::default(),
            bot_id: Default::default(),
            name: Default::default(),
            description: Default::default(),
            options: Default::default(),
        }
    }
}

impl BotCommand {
    /// Create a new bot command
    pub async fn create(
        db: &Database,
        bot_id: String,
        name: String,
        description: String,
        options: Vec<CommandOption>,
    ) -> Result<BotCommand> {
        let command = BotCommand {
            id: Ulid::new().to_string(),
            bot_id,
            name,
            description,
            options,
        };

        db.insert_bot_command(&command).await?;
        Ok(command)
    }

    /// Delete this command
    pub async fn delete(&self, db: &Database) -> Result<()> {
        db.delete_bot_command(&self.id).await
    }
}
