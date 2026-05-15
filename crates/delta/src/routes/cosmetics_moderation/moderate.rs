use company_config::config;
use company_database::{Database, User};
use company_result::{create_error, Result};
use rocket::serde::json::{Json, Value};
use rocket::State;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::util::cosmetics_notify::{send_moderation_notification, ModerationNotificationInput};
use crate::util::cosmetics_proxy::{proxy_patch, CosmeticsProxyError, ProxyConfig};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ModerateCosmeticRequest {
    /// Action to take: "approve", "reject", or "request_changes".
    pub action: String,
    /// Optional notes shown to the creator (rejection reason / change list).
    pub notes: Option<String>,
}

/// # Moderate MC Cosmetic
///
/// Approve / reject / request-changes on a pending cosmetic submission.
/// Privileged users only. Proxies the action to the EarthCosmetics backend
/// with a Company-minted JWT, then DMs the creator a notification from the
/// EarthCosmetics bot account.
#[openapi(tag = "Cosmetics Moderation")]
#[patch("/<id>", data = "<data>")]
pub async fn moderate_cosmetic(
    db: &State<Database>,
    user: User,
    id: String,
    data: Json<ModerateCosmeticRequest>,
) -> Result<Json<Value>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }
    let data = data.into_inner();
    if !["approve", "reject", "request_changes"].contains(&data.action.as_str()) {
        return Err(create_error!(FailedValidation {
            error: "action must be 'approve', 'reject', or 'request_changes'".to_string()
        }));
    }
    let config = config().await;
    let cfg = ProxyConfig {
        backend_url: &config.cosmetics.backend_url,
        private_key_path: &config.cosmetics.private_key_path,
        audience: &config.cosmetics.moderator_audience,
        ttl_secs: config.cosmetics.jwt_ttl_secs,
    };
    let path = format!("/admin/moderation/cosmetics/{}", urlencoding::encode(&id));
    let body = serde_json::json!({
        "action": data.action,
        "notes": data.notes,
    });
    let response = match proxy_patch(&cfg, &user.id, &path, &body).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("cosmetics proxy (moderate {}) failed: {:?}", data.action, e);
            return match e {
                CosmeticsProxyError::BackendError(s, _) if s.as_u16() == 404 => {
                    Err(create_error!(NotFound))
                }
                _ => Err(create_error!(InternalError)),
            };
        }
    };

    // Fire-and-forget notification to the creator. We extract creator_uuid +
    // displayName from the backend's response and send a DM from the
    // EarthCosmetics bot. Notification failure does NOT fail the moderation
    // action — moderators need feedback that the action succeeded even if
    // notification rigging isn't fully configured yet.
    if let Some(creator_uuid) = response.get("creatorUuid").and_then(|v| v.as_str()) {
        let display_name = response
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&id);
        let input = ModerationNotificationInput {
            creator_user_id: creator_uuid,
            cosmetic_id: &id,
            cosmetic_display_name: display_name,
            action: &data.action,
            notes: data.notes.as_deref(),
        };
        if let Err(e) = send_moderation_notification(db, input).await {
            log::warn!("send_moderation_notification failed for {}: {}", id, e);
        }
    } else {
        log::warn!(
            "cosmetics PATCH response has no creatorUuid; skipping moderation notification for {}",
            id,
        );
    }

    Ok(Json(response))
}
