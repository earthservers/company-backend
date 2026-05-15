use company_database::{Database, Decoration, DecorationStatus, User};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use super::submit::default_canvas_dimensions;

/// Request body for Studio decoration submission.
/// Accepts multiple field name variations for compatibility with the Studio app.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StudioSubmitRequest {
    /// Display name (1-64 characters)
    pub name: String,
    /// Description (0-256 characters)
    #[serde(default)]
    pub description: String,
    /// Decoration category
    pub category: String,
    /// Lottie JSON animation data (accepts "lottie_json" or "asset")
    #[serde(alias = "asset", alias = "lottie_data", alias = "animation_data")]
    pub lottie_json: serde_json::Value,
    /// Asset type hint (default: "lottie")
    #[serde(default = "default_asset_type", alias = "assetType")]
    pub asset_type: String,
    /// Whether the creator wants this to be free
    #[serde(default, alias = "creatorWantsFree", alias = "is_free")]
    pub creator_wants_free: bool,
    /// Suggested price in cents
    #[serde(default, alias = "suggestedPriceCents", alias = "price_cents")]
    pub suggested_price_cents: u32,
    /// Optional thumbnail URL or base64
    #[serde(default, alias = "thumbnail_url")]
    pub thumbnail: Option<String>,
}

fn default_asset_type() -> String {
    "lottie".to_string()
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StudioSubmitResponse {
    pub id: String,
    pub status: String,
    pub message: String,
}

/// # Studio Submit Decoration
///
/// Submit a decoration directly from the Decoration Studio.
/// Creates the decoration in the database with Pending status.
/// No Stripe payment required (Studio submissions are handled separately).
#[openapi(tag = "Decorations")]
#[post("/studio-submit", data = "<data>")]
pub async fn studio_submit_decoration(
    db: &State<Database>,
    user: User,
    data: Json<StudioSubmitRequest>,
) -> Result<Json<StudioSubmitResponse>> {
    let data = data.into_inner();

    // Validate name
    if data.name.is_empty() || data.name.len() > 64 {
        return Err(create_error!(FailedValidation {
            error: "Name must be between 1 and 64 characters".to_string()
        }));
    }

    // Validate description
    if data.description.len() > 256 {
        return Err(create_error!(FailedValidation {
            error: "Description must be at most 256 characters".to_string()
        }));
    }

    // Validate category and get canvas dimensions
    let (canvas_width, canvas_height) =
        default_canvas_dimensions(&data.category).ok_or_else(|| {
            create_error!(FailedValidation {
                error: "Invalid category. Must be one of: AvatarFrame, Banner, Badge, Nameplate, ChatBubble, CardSmall, CardLarge, UserPopout, UserPopoutMobile, ChatBackground, ChatBackgroundMobile, ProfileModal".to_string()
            })
        })?;

    // Validate Lottie JSON
    if !data.lottie_json.is_object() {
        return Err(create_error!(FailedValidation {
            error: "lottie_json must be a valid Lottie animation object".to_string()
        }));
    }

    // Extract fps and duration from Lottie data if available
    let fps = data
        .lottie_json
        .get("fr")
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(30);

    let duration_seconds = data
        .lottie_json
        .get("op")
        .and_then(|v| v.as_f64())
        .map(|op| op / fps.max(1) as f64)
        .unwrap_or(3.0);

    let decoration_id = Ulid::new().to_string();

    let decoration = Decoration {
        id: decoration_id.clone(),
        creator_id: user.id,
        name: data.name,
        description: data.description,
        asset_id: None,
        asset_type: Some(data.asset_type),
        lottie_json: Some(data.lottie_json.to_string()),
        category: match data.category.as_str() {
            "AvatarFrame" | "avatar_frame" => company_database::DecorationCategory::AvatarFrame,
            "Banner" | "banner" => company_database::DecorationCategory::Banner,
            "Badge" | "badge" => company_database::DecorationCategory::Badge,
            "Nameplate" | "nameplate" => company_database::DecorationCategory::Nameplate,
            "ChatBubble" | "chat_bubble" => company_database::DecorationCategory::ChatBubble,
            "CardSmall" | "card_small" => company_database::DecorationCategory::CardSmall,
            "CardLarge" | "card_large" => company_database::DecorationCategory::CardLarge,
            "UserPopout" | "user_popout" => company_database::DecorationCategory::UserPopout,
            "UserPopoutMobile" | "user_popout_mobile" => company_database::DecorationCategory::UserPopoutMobile,
            "ChatBackground" | "chat_background" => company_database::DecorationCategory::ChatBackground,
            "ChatBackgroundMobile" | "chat_background_mobile" => company_database::DecorationCategory::ChatBackgroundMobile,
            "ProfileModal" | "profile_modal" => company_database::DecorationCategory::ProfileModal,
            _ => company_database::DecorationCategory::Badge,
        },
        status: DecorationStatus::Pending,
        canvas_width,
        canvas_height,
        duration_seconds,
        fps,
        is_free: data.creator_wants_free,
        price_cents: data.suggested_price_cents,
        creator_wants_free: data.creator_wants_free,
        thumbnail_url: data.thumbnail,
        download_count: 0,
        active_users_count: 0,
        created_at: Timestamp::now_utc(),
        approved_at: None,
        approved_by: None,
        rejected_reason: None,
        moderator_notes: None,
    };

    db.insert_decoration(&decoration).await?;

    Ok(Json(StudioSubmitResponse {
        id: decoration_id,
        status: "Pending".to_string(),
        message: "Decoration submitted for review.".to_string(),
    }))
}
