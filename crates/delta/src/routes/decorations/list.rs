use company_database::{Database, User};
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;

use super::DecorationResponse;

/// # List Decorations
///
/// List decorations with optional filters.
#[openapi(tag = "Decorations")]
#[get("/?<status>&<category>&<creator_id>&<limit>&<offset>")]
pub async fn list_decorations(
    db: &State<Database>,
    user: User,
    status: Option<String>,
    category: Option<String>,
    creator_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Json<Vec<DecorationResponse>>> {
    let limit = limit.unwrap_or(20).min(100).max(1);
    let offset = offset.unwrap_or(0).max(0);

    let is_privileged = user.privileged;
    let is_own = creator_id.as_deref() == Some(&user.id);

    // If creator is viewing their own decorations with no status filter,
    // fetch all statuses and merge
    if is_own && status.is_none() {
        let mut all = Vec::new();
        for s in &["Pending", "Approved", "Rejected"] {
            if let Ok(mut batch) = db
                .fetch_decorations_by_status(s, category.as_deref(), creator_id.as_deref(), limit, offset)
                .await
            {
                all.append(&mut batch);
            }
        }
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.truncate(limit as usize);
        return Ok(Json(all.into_iter().map(|d| d.into()).collect()));
    }

    // Determine which status to query
    let query_status = match status.as_deref() {
        Some(s @ "Pending") | Some(s @ "Rejected") => {
            if is_privileged || is_own {
                s
            } else {
                "Approved"
            }
        }
        _ => "Approved",
    };

    log::info!(
        "list_decorations: user={} privileged={} status={:?} query_status={} creator_id={:?}",
        user.id,
        is_privileged,
        status,
        query_status,
        creator_id
    );

    let decorations = db
        .fetch_decorations_by_status(
            query_status,
            category.as_deref(),
            creator_id.as_deref(),
            limit,
            offset,
        )
        .await?;

    log::info!("list_decorations: returning {} results", decorations.len());

    Ok(Json(decorations.into_iter().map(|d| d.into()).collect()))
}
