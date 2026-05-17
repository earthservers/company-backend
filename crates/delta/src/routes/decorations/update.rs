use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateDecorationRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub lottie_json: Option<serde_json::Value>,
    pub thumbnail: Option<String>,
    pub creator_wants_free: Option<bool>,
    /// Suggested price in EarthCoins. Older clients sending
    /// `suggested_price_cents` continue to work — same numeric value.
    #[serde(default, alias = "suggested_price_cents")]
    pub suggested_price_coins: Option<u32>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateDecorationResponse {
    pub id: String,
    pub status: String,
    pub message: String,
}

/// # Update Decoration
///
/// Update a previously submitted decoration. Only the creator can update.
/// Updating the animation resets status to Pending for re-review.
#[openapi(tag = "Decorations")]
#[patch("/<id>/update", data = "<data>")]
pub async fn update_decoration(
    db: &State<Database>,
    user: User,
    id: String,
    data: Json<UpdateDecorationRequest>,
) -> Result<Json<UpdateDecorationResponse>> {
    let decoration = db.fetch_decoration(&id).await?;

    if decoration.creator_id != user.id {
        return Err(create_error!(NotPrivileged));
    }

    let data = data.into_inner();

    if let Some(ref name) = data.name {
        if name.is_empty() || name.len() > 64 {
            return Err(create_error!(FailedValidation {
                error: "Name must be between 1 and 64 characters".to_string()
            }));
        }
    }

    if let Some(ref desc) = data.description {
        if desc.len() > 256 {
            return Err(create_error!(FailedValidation {
                error: "Description must be at most 256 characters".to_string()
            }));
        }
    }

    let lottie_str = data.lottie_json.as_ref().map(|v| v.to_string());
    let needs_rereview = lottie_str.is_some();

    let fps = data.lottie_json.as_ref().and_then(|v| {
        v.get("fr").and_then(|f| f.as_f64()).map(|f| f.round() as u32)
    });
    let duration = data.lottie_json.as_ref().and_then(|v| {
        let fr = v.get("fr").and_then(|f| f.as_f64()).unwrap_or(30.0);
        v.get("op").and_then(|f| f.as_f64()).map(|op| op / fr)
    });

    db.update_decoration_content(
        &id,
        data.name.as_deref(),
        data.description.as_deref(),
        lottie_str.as_deref(),
        data.thumbnail.as_deref(),
        data.creator_wants_free,
        data.suggested_price_coins,
        fps,
        duration,
        needs_rereview,
    )
    .await?;

    Ok(Json(UpdateDecorationResponse {
        id,
        status: if needs_rereview { "Pending".to_string() } else { format!("{:?}", decoration.status) },
        message: if needs_rereview {
            "Decoration updated and resubmitted for review.".to_string()
        } else {
            "Decoration updated.".to_string()
        },
    }))
}
