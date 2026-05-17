use iso8601_timestamp::Timestamp;
use serde::{Deserialize, Serialize};

auto_derived!(
    /// Category of profile decoration
    pub enum DecorationCategory {
        AvatarFrame,
        Banner,
        Badge,
        Nameplate,
        ChatBubble,
        CardSmall,
        CardLarge,
        UserPopout,
        UserPopoutMobile,
        ChatBackground,
        ChatBackgroundMobile,
        ProfileModal,
    }

    /// Moderation status of a decoration
    pub enum DecorationStatus {
        Pending,
        Approved,
        Rejected,
    }

    /// A decoration equipped in a specific slot on a user's profile
    pub struct DecorationEquip {
        /// ID of the equipped decoration
        pub decoration_id: String,
        /// Which slot this decoration occupies
        pub slot: DecorationCategory,
    }

    /// A record of a decoration purchase
    pub struct DecorationPurchase {
        /// Unique Id
        #[serde(rename = "_id")]
        pub id: String,
        /// Buyer user ID
        pub user_id: String,
        /// Decoration purchased
        pub decoration_id: String,
        /// Price paid in EarthCoins (1 coin = $0.01 USD at the current peg)
        pub price_paid_coins: u32,
        /// When the purchase was made
        pub purchased_at: Timestamp,
    }
);

/// A profile decoration asset
///
/// Note: Cannot use auto_derived! because serde_json::Value and f64
/// do not implement Eq (required by the macro's derive list).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Decoration {
    /// Unique Id
    #[serde(rename = "_id")]
    pub id: String,
    /// User who submitted this decoration
    pub creator_id: String,
    /// Display name
    pub name: String,
    /// Description of the decoration
    pub description: String,
    /// File ID (uploaded via Autumn) — used for static assets (legacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// Asset file type: "svg", "png", "webp" (legacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_type: Option<String>,
    /// Lottie animation JSON stored as a string (avoids BSON/JSON type mismatch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lottie_json: Option<String>,
    /// Category this decoration belongs to
    pub category: DecorationCategory,
    /// Moderation status
    pub status: DecorationStatus,
    /// Canvas width in pixels
    pub canvas_width: u32,
    /// Canvas height in pixels
    pub canvas_height: u32,
    /// Animation duration in seconds
    pub duration_seconds: f64,
    /// Animation frames per second
    pub fps: u32,
    /// Whether this decoration is free
    #[serde(default)]
    pub is_free: bool,
    /// Price in EarthCoins (1 coin = $0.01 USD at the current peg).
    /// Source of truth for purchase pricing — `purchase.rs` reads this
    /// and passes it to Nexus's `/transfer` as the coin amount.
    #[serde(default)]
    pub price_coins: u32,
    /// What the creator requested (free or paid)
    #[serde(default)]
    pub creator_wants_free: bool,
    /// Thumbnail URL for marketplace browsing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// Number of times this decoration has been downloaded/acquired
    pub download_count: u64,
    /// Number of users currently equipping this decoration
    pub active_users_count: u64,
    /// When this decoration was created
    pub created_at: Timestamp,
    /// When this decoration was approved (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<Timestamp>,
    /// Who approved this decoration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    /// Reason for rejection (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_reason: Option<String>,
    /// Moderator notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderator_notes: Option<String>,
}

