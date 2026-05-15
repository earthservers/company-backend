use company_database::{util::reference::Reference, Channel, Database, Invite, Member, User, AMQP};
use company_models::v0::{self, InviteJoinResponse};
use company_result::{create_error, Result};
use rocket::{serde::json::Json, State};

/// # Join Invite
///
/// Join an invite by its ID
#[openapi(tag = "Invites")]
#[post("/<target>")]
pub async fn join(
    db: &State<Database>,
    amqp: &State<AMQP>,
    user: User,
    target: Reference<'_>,
) -> Result<Json<v0::InviteJoinResponse>> {
    if user.bot.is_some() {
        return Err(create_error!(IsBot));
    }

    user.can_acquire_server(db).await?;

    let invite = target.as_invite(db).await?;
    match &invite {
        Invite::Server { server, .. } => {
            let server = db.fetch_server(server).await?;

            // Check member limit based on server owner's subscription tier
            let owner = db.fetch_user(&server.owner).await?;
            let max_members = owner.max_server_members().await;
            let current_members = db.fetch_member_count(&server.id).await?;
            if current_members >= max_members {
                return Err(create_error!(ServerMemberLimitReached {
                    max: max_members,
                    message: format!(
                        "This server has reached its member limit of {}. The server owner needs to upgrade their subscription to allow more members.",
                        max_members
                    )
                }));
            }

            let (_, channels) = Member::create(db, &server, &user, None).await?;

            Ok(Json(InviteJoinResponse::Server {
                channels: channels.into_iter().map(|c| c.into()).collect(),
                server: server.into(),
            }))
        }
        Invite::Group {
            channel, creator, ..
        } => {
            let mut channel = db.fetch_channel(channel).await?;
            channel.add_user_to_group(db, amqp, &user, creator).await?;
            if let Channel::Group { recipients, .. } = &channel {
                Ok(Json(InviteJoinResponse::Group {
                    users: User::fetch_many_ids_as_mutuals(db, &user, recipients).await?,
                    channel: channel.into(),
                }))
            } else {
                unreachable!()
            }
        }
    }
}
