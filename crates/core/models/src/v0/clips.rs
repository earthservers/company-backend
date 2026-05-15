use iso8601_timestamp::Timestamp;

auto_derived!(
    /// Stream clip
    pub struct Clip {
        /// Unique Id (ULID)
        #[cfg_attr(feature = "serde", serde(rename = "_id"))]
        pub id: String,

        /// ID of the stream session this clip was created from
        pub stream_session_id: String,
        /// Channel ID where the stream was happening
        pub channel_id: String,
        /// Server ID (if the stream was in a server)
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        pub server_id: Option<String>,

        /// User ID of who created the clip
        pub creator_id: String,
        /// User ID of who was streaming
        pub streamer_id: String,

        /// When the clip was created
        pub created_at: Timestamp,
        /// Clip title
        pub title: String,
        /// Optional clip description
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        pub description: Option<String>,

        /// Duration of the clip in seconds (60, 300, 600, 900, 3600)
        pub duration_seconds: u32,
        /// Offset from stream start in seconds
        pub start_offset_seconds: u64,

        /// File size in bytes
        pub file_size_bytes: u64,
        /// URL to the clip file in S3/Backblaze
        pub file_url: String,
        /// URL to the clip thumbnail
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        pub thumbnail_url: Option<String>,

        /// Number of views
        #[cfg_attr(feature = "serde", serde(default))]
        pub view_count: u32,
        /// Last time the clip was viewed
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        pub last_viewed_at: Option<Timestamp>,

        /// Auto-delete date based on retention policy (None = permanent)
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        pub retention_until: Option<Timestamp>,
        /// Whether this clip has permanent storage (Ultra tier)
        #[cfg_attr(feature = "serde", serde(default))]
        pub is_permanent: bool,

        /// Whether the clip has been soft-deleted
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        pub deleted: Option<bool>,
    }

    /// Information required to create a clip
    #[cfg_attr(feature = "validator", derive(validator::Validate))]
    pub struct DataCreateClip {
        /// Stream session ID to clip from
        pub stream_session_id: String,
        /// Channel ID
        pub channel_id: String,
        /// Clip title
        #[cfg_attr(feature = "validator", validate(length(min = 1, max = 128)))]
        pub title: String,
        /// Optional description
        #[cfg_attr(feature = "validator", validate(length(min = 0, max = 1024)))]
        pub description: Option<String>,
        /// Desired clip duration in seconds
        pub duration_seconds: u32,
        /// Offset from stream start in seconds
        pub start_offset_seconds: u64,
    }
);
