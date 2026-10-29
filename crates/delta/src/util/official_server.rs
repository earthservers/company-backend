//! The Company Official server: every user is a member.
//!
//! `api.users.official_server` in the config names the server. New users
//! join when they finish onboarding; users from before the setting join
//! once, when an admin runs `POST /admin/official-server/backfill`. Nobody
//! is put back after leaving. Bots and banned users are left alone, and the
//! server owner's member limit doesn't apply: the official server is the
//! company's.

use company_config::config;
use company_database::{Database, Member, User};

/// The configured official server id, if the setting is on.
pub async fn official_server_id() -> Option<String> {
    match config().await.api.users.official_server.clone() {
        Some(id) if !id.trim().is_empty() => Some(id),
        _ => None,
    }
}

/// What happened to one user.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Joined,
    AlreadyMember,
    Skipped,
    Failed,
}

/// Makes `user` a member of the official server if there is one and they
/// aren't one yet. Never fails the caller: problems are logged.
pub async fn ensure_official_membership(db: &Database, user: &User) -> Outcome {
    let Some(server_id) = official_server_id().await else {
        return Outcome::Skipped;
    };
    join_official_server(db, &server_id, user).await
}

/// The same for a known server id (the backfill fetches it once per user all the same: cheap and simple).
pub async fn join_official_server(db: &Database, server_id: &str, user: &User) -> Outcome {
    if user.bot.is_some() {
        return Outcome::Skipped;
    }
    if db.fetch_member(server_id, &user.id).await.is_ok() {
        return Outcome::AlreadyMember;
    }
    if db.fetch_ban(server_id, &user.id).await.is_ok() {
        return Outcome::Skipped;
    }
    let server = match db.fetch_server(server_id).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "official server {} can't be fetched (api.users.official_server): {:?}",
                server_id,
                e
            );
            return Outcome::Failed;
        }
    };
    match Member::create(db, &server, user, None).await {
        Ok(_) => {
            log::info!("user {} joined the official server {}", user.id, server_id);
            Outcome::Joined
        }
        Err(e) => {
            log::warn!(
                "user {} couldn't join the official server {}: {:?}",
                user.id,
                server_id,
                e
            );
            Outcome::Failed
        }
    }
}
