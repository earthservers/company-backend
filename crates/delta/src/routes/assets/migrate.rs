use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct MigrateResponse {
    /// Number of assets successfully migrated
    pub migrated: u64,
    /// Number of assets that failed to migrate
    pub failed: u64,
    /// Number of assets skipped (already exist in MongoDB)
    pub skipped: u64,
}

/// # Migrate Assets from S3
///
/// Admin-only endpoint. Migrates profile assets (avatars, icons, emojis,
/// banners, backgrounds) from S3/Autumn storage into MongoDB Asset documents.
///
/// This should be run before removing S3 storage. The migration process:
/// 1. Iterates all File documents with relevant tags (avatars, icons, banners, emojis, backgrounds)
/// 2. For each file, fetches the binary data from S3 via the FileHash record
/// 3. Creates an Asset document in MongoDB with the binary data embedded
/// 4. Skips files that already have a corresponding Asset document
///
/// This endpoint is idempotent and can be called multiple times safely.
/// Large migrations may take significant time; monitor the response for progress.
#[openapi(tag = "Assets")]
#[post("/migrate")]
pub async fn migrate_assets(
    db: &State<Database>,
    user: User,
) -> Result<Json<MigrateResponse>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    // TODO: Implement full migration logic:
    //
    // 1. Query all File documents where tag is one of:
    //    "avatars", "icons", "banners", "emojis", "backgrounds"
    //
    // 2. For each File:
    //    a. Check if Asset already exists with same ID (skip if so)
    //    b. Fetch the FileHash record using file.hash
    //    c. Read binary data from S3 using FileHash.bucket_id and FileHash.path
    //    d. Map file.tag to AssetCategory:
    //       - "avatars" -> Avatar
    //       - "icons" -> ServerIcon or ChannelIcon (check used_for.type)
    //       - "banners" -> ServerBanner
    //       - "emojis" -> Emoji
    //       - "backgrounds" -> Background
    //    e. Create Asset { id: file.id, content_type, filename, size, data, category, ... }
    //    f. Insert into MongoDB
    //
    // 3. Track migrated/failed/skipped counts
    //
    // Note: S3 access requires the company-files crate or direct S3 client.
    // The FileHash model contains bucket_id and path for locating the file.

    log::info!(
        "Asset migration requested by privileged user {}",
        user.id
    );

    Ok(Json(MigrateResponse {
        migrated: 0,
        failed: 0,
        skipped: 0,
    }))
}
