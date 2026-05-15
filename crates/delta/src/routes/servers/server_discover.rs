use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, JsonSchema)]
pub struct DiscoverServerEntry {
    /// Server ID
    pub id: String,
    /// Server name
    pub name: String,
    /// Server description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Icon file ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_id: Option<String>,
    /// Banner file ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_id: Option<String>,
    /// Member count (approximated by channel count for now)
    pub member_count: usize,
    /// Whether this server is featured
    pub featured: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DiscoverResponse {
    pub featured: Vec<DiscoverServerEntry>,
    pub public: Vec<DiscoverServerEntry>,
}

/// # Discover Servers
///
/// List featured and public (discoverable) servers.
#[openapi(tag = "Server Discovery")]
#[get("/discover?<limit>&<offset>")]
pub async fn discover_servers(
    db: &State<Database>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Json<DiscoverResponse>> {
    let limit = limit.unwrap_or(50).min(100).max(1);
    let offset = offset.unwrap_or(0).max(0);

    // Fetch featured server IDs
    let featured_ids = db.fetch_featured_server_ids().await?;

    // Fetch featured servers
    let featured_servers = if !featured_ids.is_empty() {
        db.fetch_servers(&featured_ids).await.unwrap_or_default()
    } else {
        vec![]
    };

    let featured_set: std::collections::HashSet<&str> =
        featured_ids.iter().map(|s| s.as_str()).collect();

    let featured: Vec<DiscoverServerEntry> = featured_servers
        .iter()
        .map(|s| DiscoverServerEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            icon_id: s.icon.as_ref().map(|f| f.id.clone()),
            banner_id: s.banner.as_ref().map(|f| f.id.clone()),
            member_count: s.channels.len(),
            featured: true,
        })
        .collect();

    // Fetch public (discoverable) servers
    let public_servers = db
        .fetch_discoverable_servers(limit, offset)
        .await?;

    let public: Vec<DiscoverServerEntry> = public_servers
        .into_iter()
        .filter(|s| !featured_set.contains(s.id.as_str()))
        .map(|s| DiscoverServerEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            icon_id: s.icon.as_ref().map(|f| f.id.clone()),
            banner_id: s.banner.as_ref().map(|f| f.id.clone()),
            member_count: s.channels.len(),
            featured: false,
        })
        .collect();

    Ok(Json(DiscoverResponse { featured, public }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetFeaturedRequest {
    /// Whether to feature this server
    pub featured: bool,
}

/// # Set Server Featured
///
/// Feature or unfeature a server. Requires privileged user.
#[openapi(tag = "Server Discovery")]
#[post("/discover/<server_id>/featured", data = "<data>")]
pub async fn set_server_featured(
    db: &State<Database>,
    user: User,
    server_id: String,
    data: Json<SetFeaturedRequest>,
) -> Result<Json<serde_json::Value>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    // Verify server exists
    let _ = db.fetch_server(&server_id).await?;

    db.set_server_featured(&server_id, data.featured).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}
