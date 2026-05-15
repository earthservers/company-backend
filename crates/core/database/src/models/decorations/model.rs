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
        /// Price paid in cents
        pub price_paid_cents: u32,
        /// When the purchase was made
        pub purchased_at: Timestamp,
    }

    /// Creator earnings from a single sale
    pub struct CreatorEarnings {
        /// Unique Id
        #[serde(rename = "_id")]
        pub id: String,
        /// Creator user ID
        pub creator_id: String,
        /// Decoration sold
        pub decoration_id: String,
        /// Reference to the purchase
        pub sale_id: String,
        /// Full sale amount in cents
        pub gross_amount_cents: u32,
        /// Platform fee in cents (40%)
        pub platform_fee_cents: u32,
        /// Creator payout in cents (60%)
        pub net_amount_cents: u32,
        /// When this earning was recorded
        pub created_at: Timestamp,
    }

    /// Creator's aggregated balance
    pub struct CreatorBalance {
        /// Creator user ID (primary key)
        #[serde(rename = "_id")]
        pub creator_id: String,
        /// Available balance for cashout in cents
        #[serde(default)]
        pub available_balance_cents: i64,
        /// Total lifetime earnings in cents
        #[serde(default)]
        pub lifetime_earnings_cents: i64,
        /// Total lifetime cashouts in cents
        #[serde(default)]
        pub lifetime_cashouts_cents: i64,
        /// Last cashout timestamp
        #[serde(skip_serializing_if = "Option::is_none")]
        pub last_cashout_at: Option<Timestamp>,
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
    /// Price in cents (0 = free)
    #[serde(default)]
    pub price_cents: u32,
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

/// A cashout request from a creator
///
/// Note: Cannot use auto_derived! because serde_json::Value does not implement Eq.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CashoutRequest {
    /// Unique Id
    #[serde(rename = "_id")]
    pub id: String,
    /// Creator user ID
    pub creator_id: String,
    /// Amount to cash out in cents
    pub amount_cents: u32,
    /// Status: pending, processing, completed, rejected
    pub status: String,
    /// Payment method: paypal, stripe, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method: Option<String>,
    /// Payment details as JSON string (e.g. email for PayPal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_details: Option<String>,
    /// When this request was created
    pub created_at: Timestamp,
    /// When this request was processed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<Timestamp>,
}
