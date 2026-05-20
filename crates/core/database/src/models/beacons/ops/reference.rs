use iso8601_timestamp::Timestamp;
use company_result::Result;

use crate::ReferenceDb;
use crate::{Beacon, PartialBeacon};

use super::AbstractBeacons;

#[async_trait]
impl AbstractBeacons for ReferenceDb {
    async fn insert_beacon(&self, _beacon: &Beacon) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn fetch_beacon(&self, _id: &str) -> Result<Beacon> {
        Err(create_error!(NotFound))
    }

    async fn fetch_beacons_by_owner(&self, _owner_user_id: &str) -> Result<Vec<Beacon>> {
        Ok(vec![])
    }

    async fn update_beacon(&self, _id: &str, _partial: &PartialBeacon) -> Result<()> {
        Err(create_error!(NotFound))
    }

    async fn record_beacon_download(&self, _id: &str, _at: Timestamp) -> Result<()> {
        Err(create_error!(NotFound))
    }
}
