use company_database::Database;
use company_result::Result;
use rocket::serde::json::Json;
use rocket::State;

use super::{decoration_response_with_lottie, DecorationResponse};

/// # Fetch Decoration
///
/// Fetch a decoration by its ID. Includes full Lottie JSON data.
#[openapi(tag = "Decorations")]
#[get("/<id>")]
pub async fn fetch_decoration(
    db: &State<Database>,
    id: String,
) -> Result<Json<DecorationResponse>> {
    let decoration = db.fetch_decoration(&id).await?;
    Ok(Json(decoration_response_with_lottie(decoration)))
}
