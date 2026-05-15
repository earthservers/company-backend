use iso8601_timestamp::Timestamp;
use company_result::Result;

use crate::{CashoutRequest, CreatorBalance, CreatorEarnings, Decoration, DecorationPurchase};

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractDecorations: Sync + Send {
    /// Insert a new decoration into the database
    async fn insert_decoration(&self, decoration: &Decoration) -> Result<()>;

    /// Fetch a decoration by its id
    async fn fetch_decoration(&self, id: &str) -> Result<Decoration>;

    /// Fetch decorations filtered by status, optional category and creator, with pagination
    async fn fetch_decorations_by_status(
        &self,
        status: &str,
        category: Option<&str>,
        creator_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Decoration>>;

    /// Update the moderation status of a decoration
    async fn update_decoration_status(
        &self,
        id: &str,
        status: &str,
        rejected_reason: Option<&str>,
        approved_at: Option<Timestamp>,
        approved_by: Option<&str>,
        moderator_notes: Option<&str>,
        price_cents: Option<u32>,
        is_free: Option<bool>,
    ) -> Result<()>;

    /// Update a decoration's content (name, description, lottie_json, thumbnail, pricing)
    /// and optionally reset status to Pending for re-review
    async fn update_decoration_content(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        lottie_json: Option<&str>,
        thumbnail_url: Option<&str>,
        creator_wants_free: Option<bool>,
        price_cents: Option<u32>,
        fps: Option<u32>,
        duration_seconds: Option<f64>,
        reset_to_pending: bool,
    ) -> Result<()>;

    /// Increment a numeric counter field on a decoration
    async fn increment_decoration_counter(&self, id: &str, field: &str) -> Result<()>;

    /// Decrement a numeric counter field on a decoration
    async fn decrement_decoration_counter(&self, id: &str, field: &str) -> Result<()>;

    // ── Marketplace operations ──

    /// Insert a decoration purchase record
    async fn insert_decoration_purchase(&self, purchase: &DecorationPurchase) -> Result<()>;

    /// Check if a user owns a specific decoration
    async fn user_owns_decoration(&self, user_id: &str, decoration_id: &str) -> Result<bool>;

    /// Fetch all decorations owned by a user
    async fn fetch_user_owned_decorations(&self, user_id: &str) -> Result<Vec<DecorationPurchase>>;

    /// Insert a creator earnings record
    async fn insert_creator_earnings(&self, earnings: &CreatorEarnings) -> Result<()>;

    /// Fetch creator earnings with optional decoration filter
    async fn fetch_creator_earnings(
        &self,
        creator_id: &str,
        decoration_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CreatorEarnings>>;

    /// Get or create creator balance
    async fn fetch_creator_balance(&self, creator_id: &str) -> Result<CreatorBalance>;

    /// Add to a creator's available balance
    async fn add_to_creator_balance(&self, creator_id: &str, amount_cents: i64) -> Result<()>;

    /// Insert a cashout request and deduct from available balance
    async fn insert_cashout_request(&self, request: &CashoutRequest) -> Result<()>;

    /// Fetch cashout requests for a creator
    async fn fetch_cashout_requests(
        &self,
        creator_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CashoutRequest>>;
}
