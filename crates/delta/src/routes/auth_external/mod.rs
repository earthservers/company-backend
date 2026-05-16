//! External-service auth endpoints.
//!
//! `POST /external-token`  — exchange a Company session for an audience-scoped
//!                            user JWT. Also writes the parent-domain SSO cookie
//!                            so subsequent calls work via cookie auth.
//! `POST /service-token`   — client-credentials grant for backend-to-backend
//!                            reads (e.g. EarthSocial hydrating its user mirror).
//! `GET  /events/users`    — server-sent stream of user-update events for the
//!                            mirror. Service-token auth.
//!
//! All endpoints reuse the cosmetics RS256 keypair; audience is the only
//! scoping mechanism. These routes are NOT part of the public OpenAPI spec —
//! they're internal-use auth and are mounted separately from `/auth` in the
//! routes module so they don't pollute the SDK.
//!
//! See `util::external_auth` for the JWT primitives and request guards.

use rocket::Route;

mod events_users;
mod external_token;
mod pairing_attest;
mod service_token;

pub fn routes() -> Vec<Route> {
    routes![
        external_token::external_token,
        service_token::service_token,
        events_users::user_events,
        pairing_attest::pairing_attest,
    ]
}
