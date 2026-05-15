use iso8601_timestamp::Timestamp;

auto_derived!(
    /// Binary asset stored directly in MongoDB.
    /// Used for profile images (avatars, icons, emojis, banners).
    /// Replaces S3/Autumn storage for identity-level assets.
    pub struct Asset {
        /// Unique ID (same as the original File ID for migrated assets)
        #[serde(rename = "_id")]
        pub id: String,
        /// MIME content type (e.g., "image/png", "image/webp")
        pub content_type: String,
        /// Original filename
        pub filename: String,
        /// File size in bytes
        pub size: i64,
        /// Binary data stored as BSON Binary
        #[serde(with = "serde_bytes")]
        pub data: Vec<u8>,
        /// Asset category for cache headers
        pub category: AssetCategory,
        /// When this asset was uploaded/migrated
        pub created_at: Timestamp,
        /// ID of the user who uploaded this asset
        #[serde(skip_serializing_if = "Option::is_none")]
        pub uploader_id: Option<String>,
    }

    /// Category of asset (determines cache duration and size limits)
    pub enum AssetCategory {
        Avatar,
        ServerIcon,
        ServerBanner,
        ChannelIcon,
        Emoji,
        Background,
        Attachment,
    }
);
