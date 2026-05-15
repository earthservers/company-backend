use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::{serde::json::Json, State};
use serde::Serialize;

/// # Platform Statistics
#[derive(Serialize, JsonSchema)]
pub struct PlatformStats {
    /// Total registered users
    pub total_users: u64,
    /// Total servers
    pub total_servers: u64,
    /// Pending reports awaiting triage
    pub pending_reports: u64,
}

/// # Platform Stats
///
/// Fetch platform-wide statistics. Privileged users only.
#[openapi(tag = "Admin")]
#[get("/stats")]
pub async fn platform_stats(
    db: &State<Database>,
    user: User,
) -> Result<Json<PlatformStats>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let (total_users, total_servers, pending_reports) = futures::try_join!(
        db.count_users(),
        db.count_servers(),
        db.count_pending_reports(),
    )?;

    Ok(Json(PlatformStats {
        total_users,
        total_servers,
        pending_reports,
    }))
}
