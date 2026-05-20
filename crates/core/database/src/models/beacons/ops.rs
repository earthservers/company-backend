use iso8601_timestamp::Timestamp;
use company_result::Result;

use crate::{Beacon, PartialBeacon};

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractBeacons: Sync + Send {
    /// Insert a new beacon record. Caller is responsible for passing
    /// an id built with `Beacon::make_id` so repeated registration of
    /// the same (owner, hash) pair upserts cleanly.
    async fn insert_beacon(&self, beacon: &Beacon) -> Result<()>;

    /// Fetch a single beacon by its composite id.
    async fn fetch_beacon(&self, id: &str) -> Result<Beacon>;

    /// All non-deleted beacons owned by a user, newest first.
    async fn fetch_beacons_by_owner(&self, owner_user_id: &str) -> Result<Vec<Beacon>>;

    /// Apply a partial update — used for changing expiry, toggling
    /// auto_beacon, or moving the status (Active → Deleted/Expired).
    async fn update_beacon(&self, id: &str, partial: &PartialBeacon) -> Result<()>;

    /// Increment the download counter and set last_download_at.
    /// Idempotent w.r.t. status — even Deleted beacons can record
    /// "someone tried to grab this" for audit. The owner's client is
    /// expected to filter on status when displaying.
    async fn record_beacon_download(&self, id: &str, at: Timestamp) -> Result<()>;
}
