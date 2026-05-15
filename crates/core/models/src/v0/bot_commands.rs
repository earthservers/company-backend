use once_cell::sync::Lazy;
use regex::Regex;

/// Regex for valid command names (lowercase alphanumeric and hyphens)
pub static RE_COMMAND_NAME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9_-]+$").unwrap());

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
        #[cfg_attr(feature = "serde", serde(rename = "type"))]
        pub option_type: CommandOptionType,
        /// Whether this option is required
        #[cfg_attr(feature = "serde", serde(default))]
        pub required: bool,
        /// Pre-defined choices for this option
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "Option::is_none")
        )]
        pub choices: Option<Vec<CommandChoice>>,
        /// Whether this option supports autocomplete
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "crate::if_false", default)
        )]
        pub autocomplete: bool,
    }

    /// A bot slash command
    pub struct BotCommand {
        /// Command Id
        #[cfg_attr(feature = "serde", serde(rename = "_id"))]
        pub id: String,
        /// Bot Id
        pub bot_id: String,
        /// Command name (lowercase, no spaces, 1-32 chars)
        pub name: String,
        /// Command description (1-100 chars)
        pub description: String,
        /// Command options (arguments)
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Vec::is_empty")
        )]
        pub options: Vec<CommandOption>,
    }

    /// Data for creating or updating a bot command
    #[derive(Default)]
    #[cfg_attr(feature = "validator", derive(validator::Validate))]
    pub struct DataCreateBotCommand {
        /// Command name (lowercase, no spaces, 1-32 chars)
        #[cfg_attr(
            feature = "validator",
            validate(length(min = 1, max = 32), regex = "super::bot_commands::RE_COMMAND_NAME")
        )]
        pub name: String,
        /// Command description
        #[cfg_attr(feature = "validator", validate(length(min = 1, max = 100)))]
        pub description: String,
        /// Command options
        #[cfg_attr(feature = "serde", serde(default))]
        pub options: Vec<CommandOption>,
    }

    /// Type of interaction
    pub enum InteractionType {
        /// Slash command invocation
        Command,
        /// Autocomplete request
        Autocomplete,
    }

    /// An option value in an interaction
    #[cfg_attr(feature = "validator", derive(validator::Validate))]
    pub struct InteractionOption {
        /// Option name
        #[cfg_attr(feature = "validator", validate(length(min = 1, max = 32)))]
        pub name: String,
        /// Option value (JSON-encoded string)
        #[cfg_attr(feature = "validator", validate(length(min = 0, max = 2000)))]
        pub value: String,
    }

    /// An interaction (slash command invocation or autocomplete request)
    pub struct Interaction {
        /// Interaction Id
        #[cfg_attr(feature = "serde", serde(rename = "_id"))]
        pub id: String,
        /// Interaction type
        #[cfg_attr(feature = "serde", serde(rename = "type"))]
        pub interaction_type: InteractionType,
        /// Command name
        pub command: String,
        /// Resolved option values
        #[cfg_attr(feature = "serde", serde(default))]
        pub options: Vec<InteractionOption>,
        /// Channel where the interaction was triggered
        pub channel_id: String,
        /// User who triggered the interaction
        pub user_id: String,
        /// Server where the interaction was triggered (if applicable)
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "Option::is_none")
        )]
        pub server_id: Option<String>,
        /// Bot Id that should handle this interaction
        pub bot_id: String,
        /// Token for responding to this interaction
        pub token: String,
    }

    /// Interaction response type
    pub enum InteractionResponseType {
        /// Acknowledge the interaction (bot will edit later)
        DeferredReply,
        /// Reply to the interaction
        Reply,
        /// Respond with autocomplete choices
        Autocomplete,
    }

    /// Data for an interaction response
    #[derive(Default)]
    pub struct InteractionResponseData {
        /// Message content
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "Option::is_none")
        )]
        pub content: Option<String>,
        /// Whether the response is only visible to the invoking user
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "Option::is_none")
        )]
        pub ephemeral: Option<bool>,
        /// Autocomplete choices (for Autocomplete response type)
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "Option::is_none")
        )]
        pub choices: Option<Vec<CommandChoice>>,
    }

    /// Interaction callback payload
    pub struct DataInteractionResponse {
        /// Response type
        #[cfg_attr(feature = "serde", serde(rename = "type"))]
        pub response_type: InteractionResponseType,
        /// Response data
        #[cfg_attr(
            feature = "serde",
            serde(skip_serializing_if = "Option::is_none")
        )]
        pub data: Option<InteractionResponseData>,
    }

    /// Data for invoking a slash command
    #[cfg_attr(feature = "validator", derive(validator::Validate))]
    pub struct DataInvokeCommand {
        /// Command ID to invoke
        #[cfg_attr(feature = "validator", validate(length(min = 1, max = 128)))]
        pub command_id: String,
        /// Option values provided by the user
        #[cfg_attr(feature = "serde", serde(default))]
        pub options: Vec<InteractionOption>,
    }

    /// Response returned when a slash command is invoked
    pub struct InvokeCommandResponse {
        /// Interaction ID
        pub id: String,
    }
);
