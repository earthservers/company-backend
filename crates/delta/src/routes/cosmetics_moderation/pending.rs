use company_config::config;
use company_database::User;
use company_result::{create_error, Result};
use rocket::serde::json::{Json, Value};

use crate::util::cosmetics_proxy::{proxy_get, CosmeticsProxyError, ProxyConfig};

/// # List Pending MC Cosmetic Submissions
///
/// Returns submissions in Pending or ChangesRequested status from the
/// EarthCosmetics backend. Privileged users only.
#[openapi(tag = "Cosmetics Moderation")]
#[get("/pending")]
pub async fn list_pending_cosmetics(user: User) -> Result<Json<Value>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }
    let config = config().await;
    let cfg = ProxyConfig {
        backend_url: &config.cosmetics.backend_url,
        private_key_path: &config.cosmetics.private_key_path,
        audience: &config.cosmetics.moderator_audience,
        ttl_secs: config.cosmetics.jwt_ttl_secs,
    };
    match proxy_get(&cfg, &user.id, "/admin/moderation/pending").await {
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
