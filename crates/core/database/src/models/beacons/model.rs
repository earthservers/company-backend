use iso8601_timestamp::Timestamp;

auto_derived!(
    /// Lifecycle state of a beacon entry. `Deleted` is a soft-delete so
    /// the row sticks around for stats / audit, but the owner's client
    /// stops re-hosting it.
    pub enum BeaconStatus {
        Active,
        Deleted,
        Expired,
    }

    /// How the owner asked the server to keep this beacon alive. The
    /// server doesn't enforce the rule — it's a hint the owner's client
    /// reads back on boot to know what to re-host. `Custom` stores the
    /// actual deadline in `expires_at`.
    pub enum BeaconExpiryMode {
        /// Active only while the tab is open; gone after refresh.
        /// (Server still has the row so we can show it in history.)
        Ephemeral,
        /// Active whenever the owner is signed in. No deadline.
        AlwaysOnline,
        /// Active until `expires_at`.
        Custom,
    }
);

auto_derived_partial!(
    /// Beacon record: server-side persistent state for a P2P file the
    /// owner has shared. Operational fields only (lifecycle, expiry,
    /// download stats). The user's *sharing graph* (which channels the
    /// beacon was posted in) stays client-side in the OPFS manifest and
    /// is intentionally NOT mirrored here.
    pub struct Beacon {
        /// Composite key: `{owner_user_id}:{file_hash}`. Lets the same
        /// user re-register the same file idempotently and lets two
        /// users share independently of each other's records.
        #[serde(rename = "_id")]
        pub id: String,
        /// User that owns this beacon (the one hosting the bytes).
        pub owner_user_id: String,
        /// SHA-256 of the file contents.
        pub file_hash: String,
        /// Lifecycle state.
        pub status: BeaconStatus,
        /// Hint about how long the owner wants to keep hosting.
        pub expiry_mode: BeaconExpiryMode,
        /// Deadline when `expiry_mode == Custom`.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub expires_at: Option<Timestamp>,
        /// Re-register this beacon on the owner's next boot.
        pub auto_beacon: bool,
        /// Times a peer completed a download of this beacon.
        pub download_count: u64,
        /// Last successful peer download.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub last_download_at: Option<Timestamp>,
        /// When this beacon was first registered.
        pub created_at: Timestamp,
    },
    "PartialBeacon"
);

impl Beacon {
    /// Build the composite document id from the owner + file hash.
    pub fn make_id(owner_user_id: &str, file_hash: &str) -> String {
        format!("{}:{}", owner_user_id, file_hash)
    }
}
