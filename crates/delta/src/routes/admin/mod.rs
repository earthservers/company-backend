use revolt_rocket_okapi::revolt_okapi::openapi3::OpenApi;
use rocket::Route;

mod reports;
mod stats;
mod users;

pub fn routes() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        reports::list_reports,
        reports::resolve_report,
        reports::reject_report,
        users::ban_user,
        users::unsuspend_user,
        stats::platform_stats,
    ]
}
