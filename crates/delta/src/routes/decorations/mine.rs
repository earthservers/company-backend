use company_database::{Database, User};
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;

use super::DecorationResponse;

/// # List My Decorations
///
/// Returns all decorations submitted by the authenticated user (any status).
#[openapi(tag = "Decorations")]
#[get("/mine?<limit>&<offset>")]
pub async fn list_my_decorations(
    db: &State<Database>,
    user: User,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Json<Vec<DecorationResponse>>> {
    let limit = limit.unwrap_or(100).min(200).max(1);
    let offset = offset.unwrap_or(0).max(0);

    let mut all = Vec::new();
    for status in &["Pending", "Approved", "Rejected"] {
        if let Ok(mut batch) = db
            .fetch_decorations_by_status(status, None, Some(&user.id), limit, offset)
            .await
        {
            all.append(&mut batch);
        }
    }

    all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all.truncate(limit as usize);

    Ok(Json(all.into_iter().map(|d| d.into()).collect()))
}
