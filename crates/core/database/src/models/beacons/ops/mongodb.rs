use bson::{doc, to_bson};
use iso8601_timestamp::Timestamp;
use mongodb::options::{FindOptions, UpdateOptions};
use company_result::Result;

use crate::MongoDb;
use crate::{Beacon, PartialBeacon};

use super::AbstractBeacons;

static COL: &str = "beacons";

#[async_trait]
impl AbstractBeacons for MongoDb {
    async fn insert_beacon(&self, beacon: &Beacon) -> Result<()> {
        // Idempotent register: a user closing the tab and rebeaconing
        // the same file shouldn't create a duplicate row. Upsert by the
        // composite id, preserving any existing download counters.
        let mut set = match to_bson(beacon) {
            Ok(bson::Bson::Document(doc)) => doc,
            _ => return Err(create_database_error!("to_bson", COL)),
        };
        // The owner shouldn't be able to roll their own counters back
        // by re-registering. Drop the counter fields and let the
        // existing values (if any) survive via $setOnInsert.
        let count_seed = set.remove("download_count").unwrap_or_else(|| 0_i64.into());
        let last_at = set.remove("last_download_at");

        let mut on_insert = doc! {
            "download_count": count_seed,
        };
        if let Some(v) = last_at {
            on_insert.insert("last_download_at", v);
        }

        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": &beacon.id },
                doc! {
                    "$set": set,
                    "$setOnInsert": on_insert,
                },
            )
            .with_options(UpdateOptions::builder().upsert(true).build())
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }

    async fn fetch_beacon(&self, id: &str) -> Result<Beacon> {
        query!(self, find_one_by_id, COL, id)?.ok_or_else(|| create_error!(NotFound))
    }

    async fn fetch_beacons_by_owner(&self, owner_user_id: &str) -> Result<Vec<Beacon>> {
        let filter = doc! {
            "owner_user_id": owner_user_id,
            "status": { "$ne": "Deleted" },
        };
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        query!(self, find_with_options, COL, filter, options)
    }

    async fn update_beacon(&self, id: &str, partial: &PartialBeacon) -> Result<()> {
        query!(self, update_one_by_id, COL, id, partial, vec![], None).map(|_| ())
    }

    async fn record_beacon_download(&self, id: &str, at: Timestamp) -> Result<()> {
        let at_bson = to_bson(&at).map_err(|_| create_database_error!("to_bson", COL))?;
        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": id },
                doc! {
                    "$inc": { "download_count": 1 },
                    "$set": { "last_download_at": at_bson },
                },
            )
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }
}
