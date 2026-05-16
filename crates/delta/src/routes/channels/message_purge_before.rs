use company_database::mongodb::bson::doc;
use company_database::{
    util::{permissions::DatabasePermissionQuery, reference::Reference},
    Channel, Database, DocumentId, User,
};
use company_permissions::{calculate_channel_permissions, ChannelPermission};
use company_result::{create_error, Result};
use company_database::mongodb::options::{
    FindOneAndUpdateOptions, FindOptions, ReturnDocument,
};
use rocket::{serde::json::Json, State};
use serde::{Deserialize, Serialize};

use crate::util::epoch_bump::mint_epoch_bump_token;
use company_database::events::client::EventV1;

/// # Purge Messages Before
///
/// Delete all messages in a channel at or before a given message ID.
///
/// For server channels, requires `ManageMessages` permission.
/// For DMs, the requesting user must be a participant.
/// For groups, the requesting user must be the owner or a moderator.
///
/// On success the server also atomically increments the channel's
/// epoch counter and returns a signed `epoch_bump_token` (RS256 JWT)
/// the caller broadcasts to the channel. Other members verify the
/// token's signature before adopting the new epoch — this is what
/// removes client-trust from the cryptographic-erasure protocol.
///
/// Returns the number of deleted messages and the signed bump token.
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
        Channel::Group { owner, moderators, recipients: _, .. } => {
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
        mongo.delete_bulk_messages(projection).await?;

        EventV1::BulkMessageDelete {
            channel: channel_id.to_string(),
            ids: deleted_ids,
        }
        .p(channel_id.to_string())
        .await;
    }

    // Atomically bump the channel epoch and mint a signed token.
    //
    // Done for every successful permission-checked purge, even when
    // `count == 0` — a no-op-server-side purge in a P2P DM should still
    // produce a valid token so the recipient can erase their local cache.
    let new_epoch = match mongo
        .col::<EpochRecord>("channel_epochs")
        .find_one_and_update(
            doc! { "_id": channel_id },
            doc! { "$inc": { "epoch": 1_i64 } },
        )
        .with_options(
            FindOneAndUpdateOptions::builder()
                .upsert(true)
                .return_document(ReturnDocument::After)
                .build(),
        )
        .await
    {
        Ok(Some(record)) => record.epoch.max(1) as u64,
        _ => 1,
    };

    let config = company_config::config().await;
    let token = match mint_epoch_bump_token(
        &config.external_auth.private_key_path,
        channel_id,
        new_epoch,
        before_id,
        &user.id,
        count as u64,
    ) {
        Ok(t) => Some(t),
        Err(err) => {
            log::warn!(
                "purge_before: failed to mint epoch bump token for channel={channel_id}: {err}"
            );
            None
        }
    };

    Ok(Json(PurgeResponse {
        count,
        new_epoch,
        epoch_bump_token: token,
    }))
}

#[derive(Serialize, Deserialize)]
struct EpochRecord {
    #[serde(rename = "_id")]
    id: String,
    epoch: i64,
}

#[derive(serde::Serialize, serde::Deserialize, revolt_rocket_okapi::JsonSchema)]
pub struct PurgeResponse {
    /// Number of messages the server actually deleted.
    pub count: usize,
    /// New channel epoch after this purge. Monotonically increasing.
    pub new_epoch: u64,
    /// Signed RS256 JWT (aud=`company-epoch-bump`) the caller relays
    /// to the channel as a `_e2e_epoch_bump` control message. Peers
    /// verify the signature before adopting the new epoch. Absent if
    /// the server failed to mint (key load / signing error).
    pub epoch_bump_token: Option<String>,
}
