//! Minecraft account-link OAuth-style flow.
//!
//! The EarthCosmetics mod (running in a player's MC client) opens a browser
//! window to `GET /oauth/link-minecraft?...` with a `redirect_uri` pointing
//! at a one-shot loopback HTTP server inside the mod. This module:
//!
//!   * **`begin`** — validates inputs, ensures the user is signed into
//!     Company, hands off to the SPA's consent UI.
//!   * **`confirm`** — auth-checks the session again, mints a short-lived
//!     RS256 JWT (aud=`earthcosmetics-link`) bound to the player's Mojang
//!     UUID, and 302s the browser to the loopback `redirect_uri` with the
//!     JWT in the URL FRAGMENT.
//!
//! Why the JWT goes in a URL fragment, not a query string: fragments are
//! never sent to the server in a redirect target's request line, so the
//! token doesn't appear in any reverse-proxy access logs along the way and
//! doesn't leak via Referer headers to subsequent loads on the loopback page.
//!
//! Mounted manually in `routes/mod.rs` (no OpenAPI — internal use only).

use rocket::Route;

mod begin;
mod confirm;
pub mod validate;

pub use validate::{LINK_AUDIENCE, LINK_JWT_TTL_SECS};

pub fn routes() -> Vec<Route> {
    routes![
        begin::link_minecraft_begin,
        confirm::link_minecraft_confirm,
    ]
}
