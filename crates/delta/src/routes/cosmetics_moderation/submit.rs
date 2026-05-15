use company_config::config;
use company_database::User;
use company_result::{create_error, Result};
use rocket::serde::json::{Json, Value};

use crate::util::cosmetics_proxy::{proxy_post, CosmeticsProxyError, ProxyConfig};

/// # Submit MC Cosmetic for Review
///
/// Forwards a cosmetic submission from Decoration Studio to the
/// EarthCosmetics backend. Any signed-in user can submit; the cosmetics
/// backend debits the submission fee and creates the row in Pending status.
/// Body is the unmodified `AdminPublishRequest` shape — Company API doesn't
/// inspect or rewrite the descriptor / visibility / bundle here.
#[openapi(tag = "Cosmetics Moderation")]
#[post("/submit", data = "<body>")]
pub async fn submit_cosmetic(user: User, body: Json<Value>) -> Result<Json<Value>> {
    let config = config().await;
    let cfg = ProxyConfig {
        backend_url: &config.cosmetics.backend_url,
        private_key_path: &config.cosmetics.private_key_path,
        audience: &config.cosmetics.submitter_audience,
        ttl_secs: config.cosmetics.jwt_ttl_secs,
    };
    let body = body.into_inner();
    match proxy_post(&cfg, &user.id, "/admin/cosmetics", &body).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            log::warn!("cosmetics proxy (submit) failed: {:?}", e);
            match e {
                CosmeticsProxyError::BackendError(s, body) => {
                    if s.as_u16() == 402 || s.as_u16() == 409 || s.as_u16() == 400 {
                        Err(create_error!(FailedValidation {
                            error: body.to_string()
                        }))
                    } else {
                        Err(create_error!(InternalError))
                    }
                }
                _ => Err(create_error!(InternalError)),
            }
        }
    }
}
