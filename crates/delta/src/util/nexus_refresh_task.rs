//! Background task that periodically refreshes user.subscription from
//! Earth Nexus. Covers the lifecycle events that can't be triggered by
//! Company itself — Stripe renewals, payment failures, and cancellations
//! the user initiated from the Stripe Customer Portal.
//!
//! Pacing:
//!   - The task wakes up every TICK_SECS (configurable indirectly via
//!     refresh_interval_secs / 10) and grabs up to BATCH_SIZE stale users.
//!   - "Stale" means `subscription.last_nexus_refresh_at` is missing OR
//!     older than `refresh_interval_secs` seconds.
//!   - Each user gets one Nexus call. The refresh function itself writes
//!     `last_nexus_refresh_at` so subsequent ticks skip them.
//!
//! Failure mode: if Nexus is down, the refresh function logs and skips
//! writing `last_nexus_refresh_at`, so users get retried next tick — no
//! exponential backoff yet because (a) the call has its own 10s timeout
//! so a hung Nexus doesn't stall the loop too long, (b) the tick rate
//! limits total request volume regardless of outcome, and (c) operators
//! will see the warning spam in logs if Nexus is dead.

use std::time::Duration;

use async_std::task::sleep;
use company_database::mongodb::bson::doc;
use company_database::mongodb::options::FindOptions;
use company_database::Database;
use futures::TryStreamExt;

use crate::util::nexus::refresh_user_subscription_from_nexus;

/// Max users we refresh per tick. Keeps Nexus from getting a thundering
/// herd at startup if there are many stale users, and bounds the worst-
/// case time spent in the loop body. Tune up if you start seeing the
/// queue drain too slowly.
const BATCH_SIZE: i64 = 50;

/// User documents are picked up off the `users` collection.
const USERS_COL: &str = "users";

/// Spawn the background refresh task. Caller is responsible for making
/// sure this only runs once per process. Returns immediately; the
/// task runs for the lifetime of the process.
pub fn spawn(db: Database, nexus_base_url: String, refresh_interval_secs: u64) {
    if nexus_base_url.trim().is_empty() {
        log::info!("nexus refresh task: disabled (nexus.base_url empty)");
        return;
    }
    // Wake-up cadence — split the configured interval into 10 ticks so
    // newly-stale users are picked up within ~1/10 of the interval at
    // worst rather than waiting a full window after just missing a tick.
    // Floor at 60s so we don't burn CPU on a misconfig.
    let tick_secs = std::cmp::max(60, refresh_interval_secs / 10);
    log::info!(
        "nexus refresh task: spawning (interval={}s, tick={}s, batch={})",
        refresh_interval_secs,
        tick_secs,
        BATCH_SIZE,
    );
    async_std::task::spawn(run(db, refresh_interval_secs, tick_secs));
}

async fn run(db: Database, refresh_interval_secs: u64, tick_secs: u64) {
    // Small initial delay so the task doesn't fight with cold-start work
    // when the process is still wiring up everything else.
    sleep(Duration::from_secs(15)).await;

    loop {
        if let Err(e) = tick(&db, refresh_interval_secs as i64).await {
            log::warn!("nexus refresh tick failed: {e}");
        }
        sleep(Duration::from_secs(tick_secs)).await;
    }
}

async fn tick(db: &Database, refresh_interval_secs: i64) -> Result<(), String> {
    let mongo = db.mongodb();
    let col = mongo.col::<company_database::mongodb::bson::Document>(USERS_COL);

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;
    let threshold = now_unix - refresh_interval_secs;

    // Match users whose mirror is stale OR who've never been refreshed.
    // We don't filter by "has stripe customer" because the mirror is the
    // source of truth post-migration — a user with no Nexus sub will be
    // upserted to Free and re-checked once per interval.
    let filter = doc! {
        "$or": [
            { "subscription.last_nexus_refresh_at": { "$lt": threshold } },
            { "subscription.last_nexus_refresh_at": { "$exists": false } },
        ]
    };
    let opts = FindOptions::builder()
        .projection(doc! { "_id": 1 })
        .limit(BATCH_SIZE)
        .build();

    let mut cursor = col
        .find(filter)
        .with_options(opts)
        .await
        .map_err(|e| format!("find users: {e}"))?;

    let mut count = 0usize;
    while let Some(d) = cursor
        .try_next()
        .await
        .map_err(|e| format!("cursor: {e}"))?
    {
        if let Ok(id) = d.get_str("_id") {
            refresh_user_subscription_from_nexus(db, id).await;
            count += 1;
        }
    }
    if count > 0 {
        log::debug!("nexus refresh tick: refreshed {count} user(s)");
    }
    Ok(())
}
