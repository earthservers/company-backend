use bson::{doc, to_bson, Bson};
use iso8601_timestamp::Timestamp;
use mongodb::options::FindOptions;
use company_result::Result;

use crate::{Decoration, DecorationPurchase};
use crate::MongoDb;

use super::AbstractDecorations;

static COL: &str = "decorations";
static COL_PURCHASES: &str = "decoration_purchases";

#[async_trait]
impl AbstractDecorations for MongoDb {
    async fn insert_decoration(&self, decoration: &Decoration) -> Result<()> {
        query!(self, insert_one, COL, &decoration).map(|_| ())
    }

    async fn fetch_decoration(&self, id: &str) -> Result<Decoration> {
        query!(self, find_one_by_id, COL, id)?.ok_or_else(|| create_error!(NotFound))
    }

    async fn fetch_decorations_by_status(
        &self,
        status: &str,
        category: Option<&str>,
        creator_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Decoration>> {
        let mut filter = doc! {
            "status": status,
        };

        if let Some(cat) = category {
            filter.insert("category", cat);
        }

        if let Some(cid) = creator_id {
            filter.insert("creator_id", cid);
        }

        let options = FindOptions::builder()
            .limit(limit)
            .skip(offset as u64)
            .sort(doc! { "created_at": -1 })
            .build();

        query!(self, find_with_options, COL, filter, options)
    }

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
    ) -> Result<()> {
        let mut set = doc! {};

        if let Some(v) = name { set.insert("name", v); }
        if let Some(v) = description { set.insert("description", v); }
        if let Some(v) = lottie_json { set.insert("lottie_json", v); }
        if let Some(v) = thumbnail_url { set.insert("thumbnail_url", v); }
        if let Some(v) = creator_wants_free {
            set.insert("creator_wants_free", v);
            set.insert("is_free", v);
        }
        if let Some(v) = price_coins { set.insert("price_coins", v as i64); }
        if let Some(v) = fps { set.insert("fps", v as i32); }
        if let Some(v) = duration_seconds { set.insert("duration_seconds", v); }

        if reset_to_pending {
            set.insert("status", "Pending");
            set.insert("approved_at", Bson::Null);
            set.insert("approved_by", Bson::Null);
            set.insert("rejected_reason", Bson::Null);
        }

        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": id },
                doc! { "$set": set },
            )
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }

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
    ) -> Result<()> {
        let mut set = doc! {
            "status": status,
        };

        if let Some(reason) = rejected_reason {
            set.insert("rejected_reason", reason);
        }

        if let Some(ts) = approved_at {
            set.insert(
                "approved_at",
                to_bson(&ts).map_err(|_| create_database_error!("to_bson", COL))?,
            );
        }

        if let Some(by) = approved_by {
            set.insert("approved_by", by);
        }

        if let Some(notes) = moderator_notes {
            set.insert("moderator_notes", notes);
        }

        if let Some(price) = price_coins {
            set.insert("price_coins", price as i64);
        }

        if let Some(free) = is_free {
            set.insert("is_free", free);
        }

        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": id },
                doc! { "$set": set },
            )
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }

    async fn increment_decoration_counter(&self, id: &str, field: &str) -> Result<()> {
        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": id },
                doc! { "$inc": { field: 1_i64 } },
            )
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }

    async fn decrement_decoration_counter(&self, id: &str, field: &str) -> Result<()> {
        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": id },
                doc! { "$inc": { field: -1_i64 } },
            )
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }

    // ── Marketplace operations ──
    //
    // Coin movement happens in Earth Nexus (`/transfer`). The Company
    // side only records ownership.

    async fn insert_decoration_purchase(&self, purchase: &DecorationPurchase) -> Result<()> {
        query!(self, insert_one, COL_PURCHASES, &purchase).map(|_| ())
    }

    async fn user_owns_decoration(&self, user_id: &str, decoration_id: &str) -> Result<bool> {
        let filter = doc! {
            "user_id": user_id,
            "decoration_id": decoration_id,
        };

        let result: Option<DecorationPurchase> = self
            .col::<DecorationPurchase>(COL_PURCHASES)
            .find_one(filter)
            .await
            .map_err(|_| create_database_error!("find_one", COL_PURCHASES))?;

        Ok(result.is_some())
    }

    async fn fetch_user_owned_decorations(&self, user_id: &str) -> Result<Vec<DecorationPurchase>> {
        let filter = doc! { "user_id": user_id };
        let options = FindOptions::builder()
            .sort(doc! { "purchased_at": -1 })
            .build();

        query!(self, find_with_options, COL_PURCHASES, filter, options)
    }
}
