use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;

mod forget;
mod list;
mod sync_drain;
mod sync_push;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        list::list,
        forget::forget,
        sync_push::sync_push,
        sync_drain::sync_drain,
    ]
}
