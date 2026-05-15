use company_database::mongodb::bson::doc;
use company_database::{
    util::{permissions::DatabasePermissionQuery, reference::Reference},
    Channel, Database, DocumentId, User,
};
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};
use company_database::mongodb::options::FindOptions;
use rocket::{serde::json::Json, State};

use company_database::events::client::EventV1;

/// # Purge Messages Before
///
/// Delete all messages in a channel at or before a given message ID.
///
/// For server channels, requires `ManageMessages` permission.
/// For DMs, the requesting user must be a participant.
/// For groups, the requesting user must be the owner or a moderator.
///
/// Returns the number of deleted messages.
#[openapi(tag = "Messaging")]
#[delete("/<target>/messages/purge/<before_id>", rank = 0)]
pub async fn purge_before(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
    before_id: &str,
) -> Result<Json<PurgeResponse>> {
    let channel = target.as_channel(db).await?;

    // Permission check depends on channel type.
    match &channel {
        Channel::SavedMessages { user: owner, .. } => {
            if *owner != user.id {
                return Err(create_error!(NotFound));
            }
        }
        Channel::DirectMessage { recipients, .. } => {
            if !recipients.contains(&user.id) {
                return Err(create_error!(NotFound));
            }
        }
        Channel::Group { owner, moderators, recipients, .. } => {
            // Only owner or moderators can purge in groups.
            if user.id != *owner && !moderators.contains(&user.id) {
                return Err(create_error!(MissingPermission {
                    permission: "ManageMessages".to_string()
                }));
            }
        }
        _ => {
            let mut query = DatabasePermissionQuery::new(db, &user).channel(&channel);
            calculate_channel_permissions(&mut query)
                .await
                .throw_if_lacking_channel_permission(ChannelPermission::ManageMessages)?;
        }
    }

    let channel_id = target.id;
    let mongo = db.mongodb();

    let projection = doc! {
        "channel": channel_id,
        "_id": { "$lte": before_id }
    };

    // Fetch IDs of messages to be deleted (for the event).
    let deleted_ids: Vec<String> = mongo
        .find_with_options::<_, DocumentId>(
            "messages",
            projection.clone(),
            FindOptions::builder()
                .projection(doc! { "_id": 1_i32 })
                .build(),
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.id)
        .collect();

    let count = deleted_ids.len();

    if count > 0 {
        // Delete messages and clean up attachments.
        mongo.delete_bulk_messages(projection).await?;

        // Emit event so connected clients update in real time.
        EventV1::BulkMessageDelete {
            channel: channel_id.to_string(),
            ids: deleted_ids,
        }
        .p(channel_id.to_string())
        .await;
    }

    Ok(Json(PurgeResponse { count }))
}

#[derive(serde::Serialize, serde::Deserialize, revolt_rocket_okapi::JsonSchema)]
pub struct PurgeResponse {
    pub count: usize,
}
