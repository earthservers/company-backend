use company_result::Result;

use crate::Asset;

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractAssets: Sync + Send {
    /// Insert an asset into the database.
    async fn insert_asset(&self, asset: &Asset) -> Result<()>;

    /// Fetch an asset by its ID.
    async fn fetch_asset(&self, id: &str) -> Result<Asset>;

    /// Delete an asset by its ID.
    async fn delete_asset(&self, id: &str) -> Result<()>;

    /// Check whether an asset exists by its ID.
    async fn asset_exists(&self, id: &str) -> Result<bool>;
}
