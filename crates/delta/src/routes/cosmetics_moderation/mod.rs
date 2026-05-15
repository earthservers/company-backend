//! MC cosmetics moderation proxy routes.
//!
//! Company API owns the moderator UI but doesn't store cosmetics data — it
//! proxies the EarthCosmetics backend with a Company-minted RS256 JWT
//! attached. Auth on the Company side is the existing privileged-user
//! check; cosmetics backend verifies the JWT signature + audience.

use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;

mod fetch;
mod moderate;
mod pending;
mod submit;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        pending::list_pending_cosmetics,
        fetch::fetch_cosmetic_for_review,
        moderate::moderate_cosmetic,
        submit::submit_cosmetic,
    ]
}
