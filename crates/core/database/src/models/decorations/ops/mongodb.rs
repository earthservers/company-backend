use bson::{doc, to_bson, Bson};
use iso8601_timestamp::Timestamp;
use mongodb::options::FindOptions;
use company_result::Result;

use crate::{CashoutRequest, CreatorBalance, CreatorEarnings, Decoration, DecorationPurchase};
use crate::MongoDb;

use super::AbstractDecorations;

static COL: &str = "decorations";
static COL_PURCHASES: &str = "decoration_purchases";
static COL_EARNINGS: &str = "creator_earnings";
static COL_BALANCES: &str = "creator_balances";
static COL_CASHOUTS: &str = "cashout_requests";

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
        price_cents: Option<u32>,
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
        if let Some(v) = price_cents { set.insert("price_cents", v as i64); }
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
        price_cents: Option<u32>,
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

        if let Some(price) = price_cents {
            set.insert("price_cents", price as i64);
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

    async fn insert_creator_earnings(&self, earnings: &CreatorEarnings) -> Result<()> {
        query!(self, insert_one, COL_EARNINGS, &earnings).map(|_| ())
    }

    async fn fetch_creator_earnings(
        &self,
        creator_id: &str,
        decoration_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CreatorEarnings>> {
        let mut filter = doc! { "creator_id": creator_id };

        if let Some(deco_id) = decoration_id {
            filter.insert("decoration_id", deco_id);
        }

        let options = FindOptions::builder()
            .limit(limit)
            .skip(offset as u64)
            .sort(doc! { "created_at": -1 })
            .build();

        query!(self, find_with_options, COL_EARNINGS, filter, options)
    }

    async fn fetch_creator_balance(&self, creator_id: &str) -> Result<CreatorBalance> {
        let filter = doc! { "_id": creator_id };

        let result: Option<CreatorBalance> = self
            .col::<CreatorBalance>(COL_BALANCES)
            .find_one(filter)
            .await
            .map_err(|_| create_database_error!("find_one", COL_BALANCES))?;

        Ok(result.unwrap_or(CreatorBalance {
            creator_id: creator_id.to_string(),
            available_balance_cents: 0,
            lifetime_earnings_cents: 0,
            lifetime_cashouts_cents: 0,
            last_cashout_at: None,
        }))
    }

    async fn add_to_creator_balance(&self, creator_id: &str, amount_cents: i64) -> Result<()> {
        self.col::<bson::Document>(COL_BALANCES)
            .update_one(
                doc! { "_id": creator_id },
                doc! {
                    "$inc": {
                        "available_balance_cents": amount_cents,
                        "lifetime_earnings_cents": amount_cents,
                    },
                    "$setOnInsert": {
                        "lifetime_cashouts_cents": 0_i64,
                    }
                },
            )
            .upsert(true)
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL_BALANCES))
    }

    async fn insert_cashout_request(&self, request: &CashoutRequest) -> Result<()> {
        // Deduct from available balance
        let result = self
            .col::<bson::Document>(COL_BALANCES)
            .update_one(
                doc! {
                    "_id": &request.creator_id,
                    "available_balance_cents": { "$gte": request.amount_cents as i64 },
                },
                doc! {
                    "$inc": {
                        "available_balance_cents": -(request.amount_cents as i64),
                        "lifetime_cashouts_cents": request.amount_cents as i64,
                    },
                    "$set": {
                        "last_cashout_at": to_bson(&request.created_at)
                            .map_err(|_| create_database_error!("to_bson", COL_BALANCES))?,
                    }
                },
            )
            .await
            .map_err(|_| create_database_error!("update_one", COL_BALANCES))?;

        if result.modified_count == 0 {
            return Err(create_error!(FailedValidation {
                error: "Insufficient balance for cashout".to_string()
            }));
        }

        query!(self, insert_one, COL_CASHOUTS, &request).map(|_| ())
    }

    async fn fetch_cashout_requests(
        &self,
        creator_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CashoutRequest>> {
        let filter = doc! { "creator_id": creator_id };
        let options = FindOptions::builder()
            .limit(limit)
            .skip(offset as u64)
            .sort(doc! { "created_at": -1 })
            .build();

        query!(self, find_with_options, COL_CASHOUTS, filter, options)
    }
}
