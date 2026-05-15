use company_result::Result;

use crate::{Asset, ReferenceDb};

use super::AbstractAssets;

#[async_trait]
impl AbstractAssets for ReferenceDb {
    /// Insert an asset into the database.
    async fn insert_asset(&self, _asset: &Asset) -> Result<()> {
        Err(create_error!(NotFound))
    }

    /// Fetch an asset by its ID.
    async fn fetch_asset(&self, _id: &str) -> Result<Asset> {
        Err(create_error!(NotFound))
    }

    /// Delete an asset by its ID.
    async fn delete_asset(&self, _id: &str) -> Result<()> {
        Err(create_error!(NotFound))
    }

    /// Check whether an asset exists by its ID.
    async fn asset_exists(&self, _id: &str) -> Result<bool> {
        Err(create_error!(NotFound))
    }
}
