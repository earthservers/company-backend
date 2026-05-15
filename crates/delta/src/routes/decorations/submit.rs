use company_config::Stripe;
use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canvas dimensions per category (accepts both PascalCase and snake_case)
pub fn default_canvas_dimensions(category: &str) -> Option<(u32, u32)> {
    match category {
        "AvatarFrame" | "avatar_frame" => Some((256, 256)),
        "Banner" | "banner" => Some((960, 320)),
        "Badge" | "badge" => Some((64, 64)),
        "ChatBubble" | "chat_bubble" => Some((320, 160)),
        "Nameplate" | "nameplate" => Some((384, 96)),
        "CardSmall" | "card_small" => Some((340, 505)),
        "CardLarge" | "card_large" => Some((340, 977)),
        "UserPopout" | "user_popout" => Some((399, 450)),
        "UserPopoutMobile" | "user_popout_mobile" => Some((340, 400)),
        "ChatBackground" | "chat_background" => Some((960, 540)),
        "ChatBackgroundMobile" | "chat_background_mobile" => Some((540, 960)),
        "ProfileModal" | "profile_modal" => Some((780, 600)),
        _ => None,
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubmitDecorationRequest {
    /// Display name (1-64 characters)
    pub name: String,
    /// Description (0-256 characters)
    pub description: String,
    /// Lottie JSON animation data
    pub lottie_json: serde_json::Value,
    /// Decoration category
    pub category: String,
    /// Animation duration in seconds
    pub duration_seconds: f64,
    /// Frames per second
    pub fps: u32,
    /// Whether the creator wants this to be free
    #[serde(default)]
    pub creator_wants_free: bool,
    /// Optional thumbnail URL
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SubmitDecorationResponse {
    /// Stripe Checkout URL to redirect user to for payment
    pub checkout_url: String,
    /// Stripe Checkout Session ID
    pub session_id: String,
}

/// # Submit Decoration
///
/// Initiate a decoration submission. Creates a Stripe Checkout Session for the $5 submission fee.
/// The decoration is created in the database only after payment succeeds (via webhook).
#[openapi(tag = "Decorations")]
#[post("/submit", data = "<data>")]
pub async fn submit_decoration(
    db: &State<Database>,
    user: User,
    data: Json<SubmitDecorationRequest>,
    stripe_config: &State<Stripe>,
) -> Result<Json<SubmitDecorationResponse>> {
    let data = data.into_inner();

    // Validate name length
    if data.name.is_empty() || data.name.len() > 64 {
        return Err(create_error!(FailedValidation {
            error: "Name must be between 1 and 64 characters".to_string()
        }));
    }

    // Validate description length
    if data.description.len() > 256 {
        return Err(create_error!(FailedValidation {
            error: "Description must be at most 256 characters".to_string()
        }));
    }

    // Validate category and get canvas dimensions
    let (canvas_width, canvas_height) = default_canvas_dimensions(&data.category).ok_or_else(|| {
        create_error!(FailedValidation {
            error: "Invalid category. Must be one of: AvatarFrame, Banner, Badge, Nameplate, ChatBubble, CardSmall, CardLarge, UserPopout, UserPopoutMobile, ChatBackground, ChatBackgroundMobile, ProfileModal".to_string()
        })
    })?;

    // Validate Lottie JSON has required fields
    if !data.lottie_json.is_object() {
        return Err(create_error!(FailedValidation {
            error: "lottie_json must be a valid Lottie animation object".to_string()
        }));
    }

    // Validate fps
    if data.fps == 0 || data.fps > 120 {
        return Err(create_error!(FailedValidation {
            error: "fps must be between 1 and 120".to_string()
        }));
    }

    // Validate duration
    if data.duration_seconds <= 0.0 || data.duration_seconds > 60.0 {
        return Err(create_error!(FailedValidation {
            error: "duration_seconds must be between 0 and 60".to_string()
        }));
    }

    let price_id = &stripe_config.price_decoration_submission;
    if price_id.is_empty() {
        return Err(create_error!(StripeError {
            message: "Decoration submission price not configured".to_string()
        }));
    }

    // Serialize the lottie_json + metadata to store after payment
    let metadata_json = serde_json::json!({
        "name": data.name,
        "description": data.description,
        "category": data.category,
        "canvas_width": canvas_width,
        "canvas_height": canvas_height,
        "duration_seconds": data.duration_seconds,
        "fps": data.fps,
        "creator_wants_free": data.creator_wants_free,
        "thumbnail_url": data.thumbnail_url,
    });

    // Create Stripe Checkout Session with decoration metadata
    let stripe_client = reqwest::Client::new();

    let success_url = format!(
        "{}&session_id={{CHECKOUT_SESSION_ID}}",
        stripe_config.success_url.replace("subscribe", "decorations")
    );

    let params = vec![
        ("mode", "payment".to_string()),
        ("line_items[0][price]", price_id.clone()),
        ("line_items[0][quantity]", "1".to_string()),
        ("success_url", success_url),
        ("cancel_url", stripe_config.cancel_url.replace("subscribe", "decorations")),
        ("client_reference_id", user.id.clone()),
        ("metadata[user_id]", user.id.clone()),
        ("metadata[product]", "decoration_submission".to_string()),
        ("metadata[decoration_metadata]", metadata_json.to_string()),
    ];

    let response = stripe_client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .header(
            "Authorization",
            format!("Bearer {}", stripe_config.secret_key),
        )
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            create_error!(StripeError {
                message: format!("Failed to create checkout session: {e}")
            })
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        log::error!("Stripe checkout error for decoration: {error_text}");
        return Err(create_error!(StripeError {
            message: "Failed to create checkout session".to_string()
        }));
    }

    let session: serde_json::Value = response.json().await.map_err(|_| {
        create_error!(StripeError {
            message: "Failed to parse Stripe response".to_string()
        })
    })?;

    let url = session["url"]
        .as_str()
        .ok_or_else(|| {
            create_error!(StripeError {
                message: "No URL in Stripe response".to_string()
            })
        })?
        .to_string();

    let session_id = session["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    Ok(Json(SubmitDecorationResponse {
        checkout_url: url,
        session_id,
    }))
}
