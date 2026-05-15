use company_database::Database;
use revolt_rocket_okapi::revolt_okapi::openapi3::{self, MediaType, RefOr};
use rocket::http::{ContentType, Header};
use rocket::response::{self, Responder, Response};
use rocket::State;
use schemars::schema::{InstanceType, SchemaObject, SingleOrVec};
use std::io::Cursor;

/// Response type that serves binary asset data with appropriate headers.
pub struct AssetResponse {
    pub data: Vec<u8>,
    pub content_type: String,
    pub filename: String,
}

impl<'r> Responder<'r, 'static> for AssetResponse {
    fn respond_to(self, _: &'r rocket::Request<'_>) -> response::Result<'static> {
        let content_type =
            ContentType::parse_flexible(&self.content_type).unwrap_or(ContentType::Binary);

        Response::build()
            .header(content_type)
            .header(Header::new(
                "Cache-Control",
                "public, max-age=604800, immutable",
            ))
            .header(Header::new("X-Content-Type-Options", "nosniff"))
            .sized_body(self.data.len(), Cursor::new(self.data))
            .ok()
    }
}

impl revolt_rocket_okapi::response::OpenApiResponderInner for AssetResponse {
    fn responses(
        _gen: &mut revolt_rocket_okapi::gen::OpenApiGenerator,
    ) -> std::result::Result<openapi3::Responses, revolt_rocket_okapi::OpenApiError> {
        let mut responses = schemars::Map::new();
        let mut content = schemars::Map::new();

        content.insert(
            "application/octet-stream".to_owned(),
            MediaType {
                schema: Some(SchemaObject {
                    instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
                    format: Some("binary".to_owned()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        responses.insert(
            "200".to_string(),
            RefOr::Object(openapi3::Response {
                description: "Binary asset data".to_string(),
                content,
                ..Default::default()
            }),
        );

        Ok(openapi3::Responses {
            responses,
            ..Default::default()
        })
    }
}

/// # Serve Asset
///
/// Serves a binary asset (avatar, icon, emoji, banner) from MongoDB.
/// Assets are cached for 7 days with immutable cache semantics.
#[openapi(tag = "Assets")]
#[get("/<id>")]
pub async fn serve_asset(
    db: &State<Database>,
    id: String,
) -> std::result::Result<AssetResponse, rocket::http::Status> {
    let asset = db
        .fetch_asset(&id)
        .await
        .map_err(|_| rocket::http::Status::NotFound)?;

    Ok(AssetResponse {
        data: asset.data,
        content_type: asset.content_type,
        filename: asset.filename,
    })
}
