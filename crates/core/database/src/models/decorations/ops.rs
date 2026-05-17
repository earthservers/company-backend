use iso8601_timestamp::Timestamp;
use company_result::Result;

use crate::{Decoration, DecorationPurchase};

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
        price_coins: Option<u32>,
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
        price_coins: Option<u32>,
        fps: Option<u32>,
        duration_seconds: Option<f64>,
        reset_to_pending: bool,
    ) -> Result<()>;

    /// Increment a numeric counter field on a decoration
    async fn increment_decoration_counter(&self, id: &str, field: &str) -> Result<()>;

    /// Decrement a numeric counter field on a decoration
    async fn decrement_decoration_counter(&self, id: &str, field: &str) -> Result<()>;

    // ── Marketplace operations ──
    //
    // Coin movement itself lives in Earth Nexus (`/transfer`). The
    // Company side only records ownership — no creator-earnings or
    // creator-balance bookkeeping here.

    /// Insert a decoration purchase record
    async fn insert_decoration_purchase(&self, purchase: &DecorationPurchase) -> Result<()>;

    /// Check if a user owns a specific decoration
    async fn user_owns_decoration(&self, user_id: &str, decoration_id: &str) -> Result<bool>;

    /// Fetch all decorations owned by a user
    async fn fetch_user_owned_decorations(&self, user_id: &str) -> Result<Vec<DecorationPurchase>>;
}
