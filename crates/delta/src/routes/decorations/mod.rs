use company_database::Decoration;
use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;
use schemars::JsonSchema;
use serde::Serialize;

mod equip;
mod fetch;
mod list;
mod moderate;
mod mine;
mod owned;
mod pending;
mod purchase;
pub mod submit;
mod studio_submit;
mod update;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        submit::submit_decoration,
        studio_submit::studio_submit_decoration,
        update::update_decoration,
        pending::list_pending_decorations,
        mine::list_my_decorations,
        list::list_decorations,
        owned::list_owned_decorations,
        fetch::fetch_decoration,
        equip::equip_decoration,
        equip::unequip_decoration,
        moderate::moderate_decoration,
        purchase::purchase_decoration,
    ]
}

/// API response type for a Decoration (derives JsonSchema for OpenAPI)
#[derive(Debug, Serialize, JsonSchema)]
pub struct DecorationResponse {
    /// Unique Id
    #[serde(rename = "_id")]
    pub id: String,
    /// User who submitted this decoration
    pub creator_id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Autumn file ID (legacy static assets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// Asset file type (legacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_type: Option<String>,
    /// Whether this decoration has Lottie animation data
    pub has_lottie: bool,
    /// Lottie JSON animation data (only included when fetching individual decoration)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lottie_json: Option<serde_json::Value>,
    /// Category
    pub category: String,
    /// Moderation status
    pub status: String,
    /// Canvas width in pixels
    pub canvas_width: u32,
    /// Canvas height in pixels
    pub canvas_height: u32,
    /// Animation duration in seconds
    pub duration_seconds: f64,
    /// Frames per second
    pub fps: u32,
    /// Whether this decoration is free
    pub is_free: bool,
    /// Price in EarthCoins (1 coin = $0.01 USD). 0 = free.
    pub price_coins: u32,
    /// Thumbnail URL for marketplace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// Download count
    pub download_count: u64,
    /// Active users count
    pub active_users_count: u64,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
    /// Approval timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
    /// Rejection reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_reason: Option<String>,
}

impl From<Decoration> for DecorationResponse {
    fn from(d: Decoration) -> Self {
        DecorationResponse {
            id: d.id,
            creator_id: d.creator_id,
            name: d.name,
            description: d.description,
            asset_id: d.asset_id,
            asset_type: d.asset_type,
            has_lottie: d.lottie_json.is_some(),
            lottie_json: None, // Not included in list responses by default
            category: format!("{:?}", d.category),
            status: format!("{:?}", d.status),
            canvas_width: d.canvas_width,
            canvas_height: d.canvas_height,
            duration_seconds: d.duration_seconds,
            fps: d.fps,
            is_free: d.is_free,
            price_coins: d.price_coins,
            thumbnail_url: d.thumbnail_url,
            download_count: d.download_count,
            active_users_count: d.active_users_count,
            created_at: d.created_at.to_string(),
            approved_at: d.approved_at.map(|t| t.to_string()),
            rejected_reason: d.rejected_reason,
        }
    }
}

/// Convert with lottie_json included (for individual fetch)
pub fn decoration_response_with_lottie(d: Decoration) -> DecorationResponse {
    let lottie: Option<serde_json::Value> = d
        .lottie_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());
    let mut resp = DecorationResponse::from(d);
    resp.lottie_json = lottie;
    resp
}
