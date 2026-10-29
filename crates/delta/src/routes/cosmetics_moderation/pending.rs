use company_config::config;
use company_database::User;
use company_result::{create_error, Result};
use rocket::serde::json::{Json, Value};

use crate::util::cosmetics_proxy::{proxy_get, CosmeticsProxyError, ProxyConfig};

/// # List MC Cosmetic Submissions
///
/// Returns submissions from the EarthCosmetics backend: those in Pending or
/// ChangesRequested status, or with `scope=all` every submission at any
/// status (approved and rejected ones can be moderated again, e.g. a
/// takedown). Privileged users only.
#[openapi(tag = "Cosmetics Moderation")]
#[get("/pending?<scope>")]
pub async fn list_pending_cosmetics(user: User, scope: Option<String>) -> Result<Json<Value>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }
    let path = match scope.as_deref() {
        None | Some("queue") => "/admin/moderation/pending".to_string(),
        Some("all") => "/admin/moderation/pending?scope=all".to_string(),
        Some(_) => {
            return Err(create_error!(FailedValidation {
                error: "scope must be 'queue' or 'all'".to_string()
            }))
        }
    };
    let config = config().await;
    let cfg = ProxyConfig {
        backend_url: &config.cosmetics.backend_url,
        private_key_path: &config.cosmetics.private_key_path,
        audience: &config.cosmetics.moderator_audience,
        ttl_secs: config.cosmetics.jwt_ttl_secs,
    };
    match proxy_get(&cfg, &user.id, &path).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            log::warn!("cosmetics proxy (pending) failed: {:?}", e);
            match e {
                CosmeticsProxyError::NotConfigured => Err(create_error!(InternalError)),
                _ => Err(create_error!(InternalError)),
            }
        }
    }
}
