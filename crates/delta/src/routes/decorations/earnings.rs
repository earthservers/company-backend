use company_database::{Database, User};
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct EarningsEntry {
    pub id: String,
    pub decoration_id: String,
    pub sale_id: String,
    pub gross_amount_cents: u32,
    pub platform_fee_cents: u32,
    pub net_amount_cents: u32,
    pub created_at: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EarningsResponse {
    pub earnings: Vec<EarningsEntry>,
}

/// # Get Creator Earnings
///
/// Fetch earnings history for the current user as a decoration creator.
#[openapi(tag = "Decorations")]
#[get("/earnings?<decoration_id>&<limit>&<offset>")]
pub async fn get_creator_earnings(
    db: &State<Database>,
    user: User,
    decoration_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Json<EarningsResponse>> {
    let limit = limit.unwrap_or(50).min(100).max(1);
    let offset = offset.unwrap_or(0).max(0);

    let earnings = db
        .fetch_creator_earnings(&user.id, decoration_id.as_deref(), limit, offset)
        .await?;

    Ok(Json(EarningsResponse {
        earnings: earnings
            .into_iter()
            .map(|e| EarningsEntry {
                id: e.id,
                decoration_id: e.decoration_id,
                sale_id: e.sale_id,
                gross_amount_cents: e.gross_amount_cents,
                platform_fee_cents: e.platform_fee_cents,
                net_amount_cents: e.net_amount_cents,
                created_at: e.created_at.to_string(),
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BalanceResponse {
    pub available_balance_cents: i64,
    pub lifetime_earnings_cents: i64,
    pub lifetime_cashouts_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cashout_at: Option<String>,
}

/// # Get Creator Balance
///
/// Fetch the current user's creator balance and lifetime stats.
#[openapi(tag = "Decorations")]
#[get("/balance")]
pub async fn get_creator_balance(
    db: &State<Database>,
    user: User,
) -> Result<Json<BalanceResponse>> {
    let balance = db.fetch_creator_balance(&user.id).await?;

    Ok(Json(BalanceResponse {
        available_balance_cents: balance.available_balance_cents,
        lifetime_earnings_cents: balance.lifetime_earnings_cents,
        lifetime_cashouts_cents: balance.lifetime_cashouts_cents,
        last_cashout_at: balance.last_cashout_at.map(|t| t.to_string()),
    }))
}
