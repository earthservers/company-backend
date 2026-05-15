use company_database::{Asset, AssetCategory, Database, File, Metadata, User};
use company_result::{create_error, Result};
use iso8601_timestamp::Timestamp;
use rocket::data::{Data, ToByteUnit};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;
use ulid::Ulid;

/// Allowed image MIME types for profile/identity asset uploads.
const ALLOWED_IMAGE_MIME_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
];

#[derive(Debug, Serialize, JsonSchema)]
pub struct UploadAssetResponse {
    /// Asset ID
    pub id: String,
}

fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" => "wav",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        "text/plain" => "txt",
        _ => "bin",
    }
}

/// Infer a Metadata variant from mime + raw bytes.
/// Falls back to Metadata::File if the bytes don't yield dimensions.
fn infer_metadata(mime: &str, data: &[u8]) -> Metadata {
    if mime.starts_with("image/") {
        if let Ok(size) = imagesize::blob_size(data) {
            return Metadata::Image {
                width: size.width as isize,
                height: size.height as isize,
            };
        }
    }
    if mime.starts_with("text/") {
        return Metadata::Text;
    }
    Metadata::File
}

/// # Upload Asset
///
/// Upload an asset. Stored directly in MongoDB.
///
/// Size limits per category:
/// - avatars: 4 MB
/// - icons: 2.5 MB
/// - banners: 6 MB
/// - emojis: 500 KB
/// - backgrounds: 6 MB
/// - attachments: 20 MB (any file type — message attachments)
///
/// The request body is the raw file data. Set the Content-Type header
/// to the correct MIME type. For attachments, optionally pass `?filename=...`
/// to preserve the original filename.
#[openapi(tag = "Assets")]
#[post("/<category>?<filename>", data = "<data>")]
pub async fn upload_asset(
    db: &State<Database>,
    user: User,
    category: String,
    filename: Option<String>,
    data: Data<'_>,
    content_type: &rocket::http::ContentType,
) -> Result<Json<UploadAssetResponse>> {
    let (cat, max_bytes, identity_only) = match category.as_str() {
        "avatars" => (AssetCategory::Avatar, 4_000_000usize, true),
        "icons" => (AssetCategory::ServerIcon, 2_500_000, true),
        "banners" => (AssetCategory::ServerBanner, 6_000_000, true),
        "emojis" => (AssetCategory::Emoji, 500_000, true),
        "backgrounds" => (AssetCategory::Background, 6_000_000, true),
        "attachments" => (AssetCategory::Attachment, 20_000_000, false),
        _ => {
            return Err(create_error!(InvalidOperation));
        }
    };

    let mime = content_type.to_string();

    // Identity assets (avatars, icons, banners, etc) must be images.
    // Message attachments accept any mime.
    if identity_only && !ALLOWED_IMAGE_MIME_TYPES.contains(&mime.as_str()) {
        return Err(create_error!(FailedValidation {
            error: "Invalid image type. Must be PNG, JPEG, GIF, WebP, or SVG.".to_string()
        }));
    }

    let bytes = data
        .open(max_bytes.bytes())
        .into_bytes()
        .await
        .map_err(|_| create_error!(FileTooLarge { max: max_bytes }))?;

    if !bytes.is_complete() {
        return Err(create_error!(FileTooLarge { max: max_bytes }));
    }

    let raw = bytes.into_inner();
    let id = Ulid::new().to_string();

    let extension = extension_for_mime(&mime);
    let default_name = format!("{}.{}", id, extension);
    let filename = filename
        .filter(|s| !s.is_empty())
        .unwrap_or(default_name);

    let metadata = infer_metadata(&mime, &raw);

    let asset = Asset {
        id: id.clone(),
        content_type: mime.clone(),
        filename: filename.clone(),
        size: raw.len() as i64,
        data: raw,
        category: cat,
        created_at: Timestamp::now_utc(),
        uploader_id: Some(user.id.clone()),
    };

    db.insert_asset(&asset).await?;

    // Every upload also needs a File row in the `attachments` collection so
    // the corresponding `File::use_*` lookup (server icon/banner, user
    // avatar/background, channel icon, emoji, message attachment) succeeds.
    // The lookup queries by `{_id, tag, used_for: {$exists: false}}`, so
    // `tag` here must match the category string used by `File::use_*`:
    //   - avatars     → use_user_avatar
    //   - icons       → use_server_icon / use_channel_icon
    //   - banners     → use_server_banner / use_user_banner
    //   - backgrounds → use_user_background
    //   - emojis      → use_emoji
    //   - attachments → use_attachment (sendMessage)
    let file = File {
        id: id.clone(),
        tag: category.clone(),
        filename,
        hash: None,
        uploaded_at: Some(Timestamp::now_utc()),
        uploader_id: Some(user.id.clone()),
        used_for: None,
        deleted: None,
        reported: None,
        metadata,
        content_type: mime,
        size: asset.size as isize,
        message_id: None,
        user_id: Some(user.id),
        server_id: None,
        object_id: None,
    };
    db.insert_attachment(&file).await?;

    Ok(Json(UploadAssetResponse { id }))
}
