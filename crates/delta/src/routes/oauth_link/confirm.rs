//! `POST /oauth/link-minecraft/confirm` — second leg of the link flow.
//!
//! The SPA's consent page submits here (same-origin, session cookie auth).
//! We re-validate every parameter (defense in depth — never trust the SPA
//! alone) then mint a short-lived RS256 JWT scoped to the link audience
//! and 302 the browser to the loopback `redirect_uri` with the JWT in the
//! URL fragment.

use company_config::config;
use rocket::form::Form;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::Redirect;
use rocket::FromForm;
use serde::Deserialize;
use url::Url;
use urlencoding::encode as urlencode;

use crate::routes::oauth_link::validate::validate_all;
use crate::routes::oauth_link::{LINK_AUDIENCE, LINK_JWT_TTL_SECS};
use crate::util::cosmetics_proxy::mint_link_jwt;
use crate::util::external_auth::CompanySession;

/// Captures the `Origin` request header for CSRF defense. Forwards (giving
/// the handler `OriginHeader(None)`) when the header is absent — the
/// handler treats that as a hard reject because a SameSite=Lax cookie
/// would otherwise let a cross-site form POST through with a missing Origin.
pub struct OriginHeader(pub Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for OriginHeader {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(OriginHeader(
            request.headers().get_one("Origin").map(|s| s.to_string()),
        ))
    }
}

/// Submitted as `application/x-www-form-urlencoded` because the SPA's
/// "Confirm" handler does a top-level form POST — the response is a 302
/// to the loopback `redirect_uri`, which only happens via real browser
/// navigation (fetch can't navigate the page, and a JSON POST followed by
/// JS-driven `window.location` would briefly expose the JWT in a fetch
/// response body in dev tools).
#[derive(Debug, Deserialize, FromForm)]
pub struct ConfirmBody {
    pub redirect_uri: String,
    pub state: String,
    pub mojang_uuid: String,
}

#[post("/oauth/link-minecraft/confirm", data = "<body>")]
pub async fn link_minecraft_confirm(
    session: CompanySession,
    origin: OriginHeader,
    body: Form<ConfirmBody>,
) -> Result<Redirect, (Status, String)> {
    let cfg = config().await;

    // CSRF defense: Origin must match the configured app host. Without
    // this, an attacker page could auto-submit a form to /confirm and the
    // SameSite=Lax session cookie would still be sent (Lax allows top-level
    // cross-site POSTs). Cosmetics-backend's mojang_uuid_intent + loopback
    // redirect_uri checks bound the damage further, but failing closed at
    // the Origin layer is the standard defense.
    //
    // Parse both as URLs and compare (scheme, host, port) so a trailing
    // slash, path component, or default-port omission in `hosts.app` doesn't
    // cause spurious mismatches. The browser-sent Origin is canonical
    // (scheme + host + optional port, no path, no trailing slash).
    let origin_ok = origin
        .0
        .as_deref()
        .and_then(|o| Url::parse(o).ok())
        .zip(Url::parse(&cfg.hosts.app).ok())
        .map(|(got, want)| {
            got.scheme() == want.scheme()
                && got.host_str() == want.host_str()
                && got.port_or_known_default() == want.port_or_known_default()
        })
        .unwrap_or(false);
    if !origin_ok {
        log::warn!(
            "link-minecraft confirm rejected — bad/missing Origin (got {:?}, want {:?})",
            origin.0,
            cfg.hosts.app
        );
        return Err((
            Status::Forbidden,
            serde_json::json!({ "error": "bad_origin" }).to_string(),
        ));
    }

    // Re-validate. The SPA is supposed to reject malformed input before
    // submitting, but a hand-crafted POST could skip that step.
    if let Err(e) = validate_all(&body.redirect_uri, &body.state, &body.mojang_uuid) {
        return Err((
            Status::BadRequest,
            serde_json::json!({ "error": e.as_str() }).to_string(),
        ));
    }
    // Reuse the cosmetics keypair — same RS256 private key signs all
    // outbound external-audience tokens. The link audience is its own
    // distinct value so cosmetics-backend can dedup and route handler
    // logic by aud.
    let private_key_path = &cfg.cosmetics.private_key_path;

    // Per-mint random nonce. nanoid gives ~126 bits of entropy in 21 chars,
    // which the cosmetics backend dedup-tracks as a single-use identifier.
    let nonce = nanoid::nanoid!();

    let jwt = match mint_link_jwt(
        &session.user_id,
        LINK_AUDIENCE,
        LINK_JWT_TTL_SECS,
        private_key_path,
        &body.mojang_uuid,
        &nonce,
    ) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("link-minecraft mint failed: {:?}", e);
            return Err((
                Status::InternalServerError,
                serde_json::json!({ "error": "mint_failed" }).to_string(),
            ));
        }
    };

    log::info!(
        "link-minecraft mint ok: user={} mojang={} (TTL {}s)",
        session.user_id,
        body.mojang_uuid,
        LINK_JWT_TTL_SECS
    );

    // Hand the JWT back to the mod via URL fragment. Fragments don't appear
    // in the request line of the browser's redirect target, so the token
    // never lands in any web-server access log between here and the mod.
    let target = format!(
        "{}#company_jwt={}&state={}",
        body.redirect_uri,
        urlencode(&jwt),
        urlencode(&body.state),
    );
    Ok(Redirect::to(target))
}

#[cfg(test)]
mod tests {
    use crate::routes::oauth_link::validate::{validate_all, LinkParamError};

    #[test]
    fn validate_all_rejects_lookalike_redirect() {
        // Same checks as begin.rs — confirm is the security boundary.
        let r = validate_all(
            "https://attacker.com/cb",
            "abcdefghij",
            "12345678-1234-1234-1234-123456789abc",
        );
        assert!(matches!(r, Err(LinkParamError::BadRedirectUri)));
    }
}
