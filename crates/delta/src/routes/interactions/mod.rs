use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;

pub mod callback;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![callback::interaction_callback,]
}
