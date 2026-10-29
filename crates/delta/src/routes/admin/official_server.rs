use company_database::{mongodb::bson::doc, Database, User};
use company_result::{create_error, Result};
use futures::StreamExt;
use rocket::{serde::json::Json, State};
use serde::Serialize;

use crate::util::official_server::{join_official_server, official_server_id, Outcome};

/// # Official Server Backfill Result
#[derive(Serialize, JsonSchema)]
pub struct OfficialServerBackfill {
    /// The server every user was added to
    pub server: String,
    /// Users seen
    pub users: usize,
    /// Users added this run
    pub joined: usize,
    /// Users who were members already
    pub already: usize,
    /// Bots and banned users
    pub skipped: usize,
    /// Users the join failed for (see the API log)
    pub failed: usize,
}

/// # Backfill the Official Server
///
/// Adds every existing user to the Company Official server
/// (`api.users.official_server`), once. New users join at onboarding on their
/// own; this is for the accounts from before the setting. Run it again any
/// time: members are left alone, and nobody who left is put back (a user
/// who left is a member no more and so would be added again: run it once).
/// Privileged users only.
#[openapi(tag = "Admin")]
#[post("/official-server/backfill")]
pub async fn backfill_official_server(
    db: &State<Database>,
    user: User,
) -> Result<Json<OfficialServerBackfill>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }
    let Some(server) = official_server_id().await else {
        return Err(create_error!(FailedValidation {
            error: "api.users.official_server is not set".to_string()
        }));
    };
    let mongo = match db.inner() {
        Database::MongoDb(m) => m,
        _ => {
            return Err(create_error!(FailedValidation {
                error: "the backfill needs the MongoDB driver".to_string()
            }))
        }
    };
    let mut out = OfficialServerBackfill {
        server: server.clone(),
        users: 0,
        joined: 0,
        already: 0,
        skipped: 0,
        failed: 0,
    };
    let mut cursor = mongo
        .col::<User>("users")
        .find(doc! { "bot": { "$exists": false } })
        .await
        .map_err(|_| create_error!(DatabaseError {
            operation: "find".to_string(),
            collection: "users".to_string()
        }))?;
    while let Some(next) = cursor.next().await {
        let Ok(u) = next else {
            out.failed += 1;
            continue;
        };
        out.users += 1;
        match join_official_server(db, &server, &u).await {
            Outcome::Joined => out.joined += 1,
            Outcome::AlreadyMember => out.already += 1,
            Outcome::Skipped => out.skipped += 1,
            Outcome::Failed => out.failed += 1,
        }
    }
    log::info!(
        "official server backfill: {} users, {} joined, {} already, {} skipped, {} failed",
        out.users,
        out.joined,
        out.already,
        out.skipped,
        out.failed
    );
    Ok(Json(out))
}
