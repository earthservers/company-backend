use iso8601_timestamp::Timestamp;
use company_result::Result;

use crate::{Decoration, DecorationPurchase};
use crate::ReferenceDb;

use super::AbstractDecorations;

#[async_trait]
impl AbstractDecorations for ReferenceDb {
    async fn insert_decoration(&self, _decoration: &Decoration) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn fetch_decoration(&self, _id: &str) -> Result<Decoration> {
        Err(create_error!(NotFound))
    }

    async fn fetch_decorations_by_status(
        &self,
        _status: &str,
        _category: Option<&str>,
        _creator_id: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Decoration>> {
        Err(create_error!(NotFound))
    }

    async fn update_decoration_status(
        &self,
        _id: &str,
        _status: &str,
        _rejected_reason: Option<&str>,
        _approved_at: Option<Timestamp>,
        _approved_by: Option<&str>,
        _moderator_notes: Option<&str>,
        _price_coins: Option<u32>,
        _is_free: Option<bool>,
    ) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn update_decoration_content(
        &self,
        _id: &str,
        _name: Option<&str>,
        _description: Option<&str>,
        _lottie_json: Option<&str>,
        _thumbnail_url: Option<&str>,
        _creator_wants_free: Option<bool>,
        _price_coins: Option<u32>,
        _fps: Option<u32>,
        _duration_seconds: Option<f64>,
        _reset_to_pending: bool,
    ) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn increment_decoration_counter(&self, _id: &str, _field: &str) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn decrement_decoration_counter(&self, _id: &str, _field: &str) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn insert_decoration_purchase(&self, _purchase: &DecorationPurchase) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn user_owns_decoration(&self, _user_id: &str, _decoration_id: &str) -> Result<bool> {
        Err(create_error!(NotFound))
    }

    async fn fetch_user_owned_decorations(&self, _user_id: &str) -> Result<Vec<DecorationPurchase>> {
        Err(create_error!(NotFound))
    }
}
