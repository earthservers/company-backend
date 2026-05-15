use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;

mod migrate;
mod serve;
mod upload;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        serve::serve_asset,
        upload::upload_asset,
        migrate::migrate_assets,
    ]
}
