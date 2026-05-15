auto_derived!(
    /// An encrypted channel invitation containing a channel key encrypted for a specific bot
    pub struct EncryptedChannelInvitation {
        /// Unique Id
        #[serde(rename = "_id")]
        pub id: String,
        /// Id of the channel this invitation is for
        pub channel_id: String,
        /// Id of the bot being invited
        pub bot_id: String,
        /// Id of the user who initiated the invite
        pub inviter_id: String,
        /// Channel key encrypted with bot's RSA public key (hex-encoded)
        pub encrypted_channel_key: String,
        /// When the invitation was created
        pub created_at: iso8601_timestamp::Timestamp,
        /// When the invitation expires (auto-expire after 24 hours)
        pub expires_at: iso8601_timestamp::Timestamp,
    }
);
