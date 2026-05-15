//! `GET /oauth/link-minecraft` — first leg of the link flow.
//!
//! Validates the inbound OAuth-style query, then either:
//!   * not signed in → 302 to the SPA's `/sign-in` page with `?next=` set
//!     so the user lands back here after auth;
//!   * signed in    → 302 to the SPA's `/link-minecraft` consent route,
//!     forwarding the same params; the SPA renders the consent UI and
//!     POSTs to `/oauth/link-minecraft/confirm` on accept.
//!
//! Why a redirect to the SPA instead of returning HTML inline: the consent
//! page's UI lives with the rest of the user-facing app, sharing layout,
//! design tokens, and auth state. The Rust handler stays a thin gate that
//! only enforces validation + auth; the SPA owns presentation.

use company_config::config;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::Redirect;
use urlencoding::encode as urlencode;

use crate::routes::oauth_link::validate::validate_all;
use crate::util::external_auth::CompanySession;

/// Optional-session guard. `from_request` for `CompanySession` forwards on
/// missing auth; we want to handle "not signed in" as a redirect, not a 401.
pub struct MaybeSession(pub Option<CompanySession>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for MaybeSession {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match CompanySession::from_request(request).await {
            Outcome::Success(s) => Outcome::Success(MaybeSession(Some(s))),
            _ => Outcome::Success(MaybeSession(None)),
        }
    }
}

#[get("/oauth/link-minecraft?<redirect_uri>&<state>&<mojang_uuid>")]
pub async fn link_minecraft_begin(
    redirect_uri: &str,
    state: &str,
    mojang_uuid: &str,
    session: MaybeSession,
) -> Result<Redirect, (Status, String)> {
    if let Err(e) = validate_all(redirect_uri, state, mojang_uuid) {
        // Bail with 400 instead of redirecting — a redirect would still
        // expose the bad input to the SPA, which would just re-fail.
        return Err((
            Status::BadRequest,
            serde_json::json!({ "error": e.as_str() }).to_string(),
        ));
    }

    let app_host = &config().await.hosts.app;
    // Trim trailing slash so concatenation produces canonical paths.
    let app_host = app_host.trim_end_matches('/');

    if session.0.is_none() {
        // Build the "come back here after sign-in" target. Re-encoding the
        // full original URL keeps redirect_uri/state/mojang_uuid intact.
        let next_path = format!(
            "/oauth/link-minecraft?redirect_uri={}&state={}&mojang_uuid={}",
            urlencode(redirect_uri),
            urlencode(state),
            urlencode(mojang_uuid),
        );
        // Sign-in lives on the SPA; once auth completes, the SPA redirects
        // to `next` which round-trips back here with the cookie set.
        let target = format!("{}/sign-in?next={}", app_host, urlencode(&next_path));
        return Ok(Redirect::to(target));
    }

    // Signed in — hand off to the SPA's consent route.
    // (LinkMinecraftPage in for-web/packages/client/src/interface/, mounted
    // at `/link-minecraft` in client/src/index.tsx.)
    let target = format!(
        "{}/link-minecraft?redirect_uri={}&state={}&mojang_uuid={}",
        app_host,
        urlencode(redirect_uri),
        urlencode(state),
        urlencode(mojang_uuid),
    );
    Ok(Redirect::to(target))
}

#[cfg(test)]
mod tests {
    use crate::routes::oauth_link::validate::LinkParamError;

    // The pure validation logic is tested in validate.rs; this module's
    // wiring is exercised by the Rocket integration tests in tests/.
    #[test]
    fn link_param_error_codes_are_stable_strings() {
        assert_eq!(LinkParamError::BadRedirectUri.as_str(), "bad_redirect_uri");
        assert_eq!(LinkParamError::BadState.as_str(), "bad_state");
        assert_eq!(LinkParamError::BadMojangUuid.as_str(), "bad_mojang_uuid");
    }
}
