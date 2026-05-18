use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;

mod turn_credentials;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![turn_credentials::turn_credentials]
}
