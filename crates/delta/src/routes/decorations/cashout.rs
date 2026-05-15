use company_database::{CashoutRequest, Database, User};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Minimum cashout amount in cents ($10)
const MIN_CASHOUT_CENTS: u32 = 1000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CashoutRequestBody {
    /// Amount to cash out in cents
    pub amount_cents: u32,
    /// Payment method: "paypal", "stripe"
    pub payment_method: String,
    /// Payment details (e.g. PayPal email)
    pub payment_details: serde_json::Value,  // Accepted as JSON, stored as string
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CashoutResponse {
    pub id: String,
    pub amount_cents: u32,
    pub status: String,
    pub created_at: String,
}

/// # Request Cashout
///
/// Request a cashout of creator earnings. Minimum $10.00 (1000 cents).
#[openapi(tag = "Decorations")]
#[post("/cashout", data = "<data>")]
pub async fn request_cashout(
    db: &State<Database>,
    user: User,
    data: Json<CashoutRequestBody>,
) -> Result<Json<CashoutResponse>> {
    let data = data.into_inner();

    if data.amount_cents < MIN_CASHOUT_CENTS {
        return Err(create_error!(FailedValidation {
            error: format!("Minimum cashout amount is ${:.2}", MIN_CASHOUT_CENTS as f64 / 100.0)
        }));
    }

    if !["paypal", "stripe"].contains(&data.payment_method.as_str()) {
        return Err(create_error!(FailedValidation {
            error: "Payment method must be 'paypal' or 'stripe'".to_string()
        }));
    }

    let now = Timestamp::now_utc();
    let request_id = Ulid::new().to_string();

    let request = CashoutRequest {
        id: request_id.clone(),
        creator_id: user.id.clone(),
        amount_cents: data.amount_cents,
        status: "pending".to_string(),
        payment_method: Some(data.payment_method),
        payment_details: Some(data.payment_details.to_string()),
        created_at: now,
        processed_at: None,
    };

    db.insert_cashout_request(&request).await?;

    Ok(Json(CashoutResponse {
        id: request_id,
        amount_cents: data.amount_cents,
        status: "pending".to_string(),
        created_at: now.to_string(),
    }))
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CashoutListEntry {
    pub id: String,
    pub amount_cents: u32,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<String>,
}

/// # List Cashout Requests
///
/// List the current user's cashout request history.
#[openapi(tag = "Decorations")]
#[get("/cashouts?<limit>&<offset>")]
pub async fn list_cashout_requests(
    db: &State<Database>,
    user: User,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Json<Vec<CashoutListEntry>>> {
    let limit = limit.unwrap_or(20).min(100).max(1);
    let offset = offset.unwrap_or(0).max(0);

    let requests = db.fetch_cashout_requests(&user.id, limit, offset).await?;

    Ok(Json(
        requests
            .into_iter()
            .map(|r| CashoutListEntry {
                id: r.id,
                amount_cents: r.amount_cents,
                status: r.status,
                payment_method: r.payment_method,
                created_at: r.created_at.to_string(),
                processed_at: r.processed_at.map(|t| t.to_string()),
            })
            .collect(),
    ))
}
