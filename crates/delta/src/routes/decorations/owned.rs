use company_database::{Database, User};
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct OwnedDecorationEntry {
    pub decoration_id: String,
    /// Price paid in EarthCoins.
    pub price_paid_coins: u32,
    pub purchased_at: String,
}

/// # List Owned Decorations
///
/// List all decorations the current user has purchased.
#[openapi(tag = "Decorations")]
#[get("/owned")]
pub async fn list_owned_decorations(
    db: &State<Database>,
    user: User,
) -> Result<Json<Vec<OwnedDecorationEntry>>> {
    let purchases = db.fetch_user_owned_decorations(&user.id).await?;

    Ok(Json(
        purchases
            .into_iter()
            .map(|p| OwnedDecorationEntry {
                decoration_id: p.decoration_id,
                price_paid_coins: p.price_paid_coins,
                purchased_at: p.purchased_at.to_string(),
            })
            .collect(),
    ))
}
