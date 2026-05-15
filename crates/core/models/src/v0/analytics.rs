/// A stream session tracks analytics for a single streaming session in a voice channel.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct StreamSession {
    /// Unique session ID (ULID)
    #[cfg_attr(feature = "serde", serde(rename = "_id"))]
    pub id: String,

    /// Channel this stream is in
    pub channel_id: String,
    /// User who started the stream
    pub streamer_id: String,
    /// Server the channel belongs to (if any)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub server_id: Option<String>,

    /// When the stream started (Unix timestamp ms)
    pub started_at: i64,
    /// When the stream ended (None if still live)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub ended_at: Option<i64>,

    /// Current number of viewers
    pub current_viewers: u32,
    /// Current number of participants
    pub current_participants: u32,
    /// Peak viewer count during this session
    pub peak_viewers: u32,
    /// Peak participant count during this session
    pub peak_participants: u32,

    /// Total stream duration in seconds (set on end)
    pub duration_seconds: u64,
    /// Average viewer count over the stream duration (set on end)
    pub average_viewers: f64,
}

/// A point-in-time snapshot of viewer/participant counts for graphing.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct ViewerSnapshot {
    /// Unix timestamp in seconds
    pub timestamp: i64,
    /// Number of viewers at this point
    pub viewers: u32,
    /// Number of participants at this point
    pub participants: u32,
}

/// Real-time metrics for the current (or most recent) stream on a channel.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct CurrentStreamMetrics {
    /// Whether the stream is currently live
    pub is_live: bool,
    /// Active session ID (if live)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub session_id: Option<String>,
    /// Current viewer count
    pub current_viewers: u32,
    /// Peak viewers this session
    pub peak_viewers: u32,
    /// When the stream started (Unix timestamp ms)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub started_at: Option<i64>,
    /// How long the stream has been running (seconds)
    pub duration_seconds: u64,
    /// Viewer count history for graphing (last 2 hours)
    pub viewer_history: Vec<ViewerSnapshot>,
}

/// Summary of a completed stream session for history view.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct StreamSessionSummary {
    /// Session ID
    pub id: String,
    /// When the stream started (Unix timestamp ms)
    pub started_at: i64,
    /// Total duration in seconds
    pub duration_seconds: u64,
    /// Peak viewer count
    pub peak_viewers: u32,
    /// Average viewers over the session
    pub average_viewers: f64,
}

/// Response type for stream history endpoint.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemas", derive(schemars::JsonSchema))]
pub struct StreamHistoryResponse {
    /// List of past stream sessions
    pub sessions: Vec<StreamSessionSummary>,
}
