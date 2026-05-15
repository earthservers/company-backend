use company_config::config;
use company_database::User;
use company_result::{create_error, Result};
use rocket::serde::json::{Json, Value};

use crate::util::cosmetics_proxy::{proxy_get, CosmeticsProxyError, ProxyConfig};

/// # Fetch Cosmetic for Review
///
/// Returns the full descriptor + bundle JSON for a cosmetic at any moderation
/// status. Used by Decoration's review mode + the moderation detail view.
/// Privileged users only.
#[openapi(tag = "Cosmetics Moderation")]
#[get("/<id>")]
pub async fn fetch_cosmetic_for_review(user: User, id: String) -> Result<Json<Value>> {
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
    let path = format!("/admin/moderation/cosmetics/{}", urlencoding::encode(&id));
    match proxy_get(&cfg, &user.id, &path).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            log::warn!("cosmetics proxy (fetch) failed: {:?}", e);
            match e {
                CosmeticsProxyError::BackendError(s, _) if s.as_u16() == 404 => {
                    Err(create_error!(NotFound))
                }
                _ => Err(create_error!(InternalError)),
            }
        }
    }
}
