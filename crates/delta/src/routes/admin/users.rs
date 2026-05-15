use company_database::{
    util::reference::Reference, AuditAction, AuditLogEntry, Database, User,
};
use company_result::{create_error, Result};
use rocket::{serde::json::Json, State};
use rocket_empty::EmptyResponse;
use serde::Deserialize;
use ulid::Ulid;

/// # Ban User Data
#[derive(Deserialize, JsonSchema)]
pub struct DataBanUser {
    /// Reason for the ban
    #[serde(default)]
    pub reason: Option<String>,
}

/// # Ban User
///
/// Platform-ban a user. Disables their account and terminates all sessions.
/// Privileged users only.
#[openapi(tag = "Admin")]
#[patch("/users/<target>/ban", data = "<data>")]
pub async fn ban_user(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
    data: Json<DataBanUser>,
) -> Result<EmptyResponse> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let data = data.into_inner();
    let mut target_user = target.as_user(db).await?;

    // Cannot ban yourself
    if target_user.id == user.id {
        return Err(create_error!(InvalidOperation));
    }

    // Cannot ban other privileged users
    if target_user.privileged {
        return Err(create_error!(InvalidOperation));
    }

    target_user.ban(db, data.reason.clone()).await?;

    // Write audit log
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.insert_audit_log(&AuditLogEntry {
        id: Ulid::new().to_string(),
        actor_id: user.id,
        action: AuditAction::BanUser,
        target_id: target_user.id,
        timestamp: now,
        reason: data.reason,
    })
    .await?;

    Ok(EmptyResponse)
}

/// # Unsuspend User
///
/// Re-enable a previously banned or suspended user's account.
/// Privileged users only.
#[openapi(tag = "Admin")]
#[patch("/users/<target>/unsuspend")]
pub async fn unsuspend_user(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
) -> Result<EmptyResponse> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let mut target_user = target.as_user(db).await?;

    target_user.unsuspend(db).await?;

    // Write audit log
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.insert_audit_log(&AuditLogEntry {
        id: Ulid::new().to_string(),
        actor_id: user.id,
        action: AuditAction::UnsuspendUser,
        target_id: target_user.id,
        timestamp: now,
        reason: None,
    })
    .await?;

    Ok(EmptyResponse)
}
