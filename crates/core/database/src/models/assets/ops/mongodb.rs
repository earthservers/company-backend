use company_result::Result;

use crate::{Asset, MongoDb};

use super::AbstractAssets;

static COL: &str = "assets";

#[async_trait]
impl AbstractAssets for MongoDb {
    /// Insert an asset into the database.
    async fn insert_asset(&self, asset: &Asset) -> Result<()> {
        query!(self, insert_one, COL, &asset).map(|_| ())
    }

    /// Fetch an asset by its ID.
    async fn fetch_asset(&self, id: &str) -> Result<Asset> {
        query!(self, find_one_by_id, COL, id)?.ok_or_else(|| create_error!(NotFound))
    }

    /// Delete an asset by its ID.
    async fn delete_asset(&self, id: &str) -> Result<()> {
        query!(self, delete_one_by_id, COL, id).map(|_| ())
    }

    /// Check whether an asset exists by its ID.
    async fn asset_exists(&self, id: &str) -> Result<bool> {
        Ok(query!(self, find_one_by_id, COL, id)?
            .map(|_: Asset| true)
            .unwrap_or(false))
    }
}
