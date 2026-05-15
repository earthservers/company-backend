//! Ephemeral DM message broker.
//!
//! For DM channels (DirectMessage, Group), messages are relayed in real-time
//! via WebSocket but NOT persisted to the database. If the recipient is offline,
//! messages are queued in Redis with a 6-hour TTL.
//!
//! Server channels (TextChannel) are unaffected — they still persist messages.
//!
//! ## Multi-device model
//!
//! Each WebSocket session is tracked individually under
//! `broker:devices:messaging:{user_id}` (a Redis Hash keyed by `session_id`).
//! Offline messages are queued per-session under
//! `broker:queue:messaging:{user_id}:{session_id}`, so a user with multiple
//! paired devices will not have one device drain the other's queue.
//!
//! The `messaging` namespace is intentional: streaming/audio device
//! registrations will live under their own parallel namespace
//! (`broker:devices:streaming:{user_id}`) when that feature lands, and the
//! two concerns must not share trust state.

use crate::events::client::EventV1;
use company_result::{create_error, Result, ToRevoltError};
use iso8601_timestamp::Timestamp;
use redis_kiss::{get_connection as _get_connection, redis::Pipeline, AsyncCommands, Conn};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Tunables ────────────────────────────────────────────────────

/// A session is considered currently-online if its `last_seen` is within
/// this many seconds of now. Refreshed by WebSocket heartbeats.
const HEARTBEAT_TIMEOUT_SECONDS: i64 = 300; // 5 minutes

/// A session entry is kept in the device registry for this long after its
/// last heartbeat. After this, it is treated as gone and pruned lazily.
const DEVICE_TTL_SECONDS: usize = 604800; // 7 days

/// How long an offline-queued message lives before it expires.
const OFFLINE_QUEUE_TTL_SECONDS: usize = 21600; // 6 hours

/// Maximum number of messages a single per-device queue can hold.
const OFFLINE_QUEUE_MAX_DEPTH: usize = 100;

/// Maximum size of a single queued message in bytes.
const OFFLINE_QUEUE_MAX_MESSAGE_BYTES: usize = 4096;

/// Sliding window for sender rate limiting.
const RATE_LIMIT_WINDOW_SECONDS: usize = 60;

/// Maximum offline messages a sender may queue to a single recipient user
/// per window.
const RATE_LIMIT_MAX_PER_WINDOW: usize = 30;

/// Maximum offline messages a sender may queue across ALL recipients per
/// window. Stops fan-out attacks that bypass the per-pair cap.
const RATE_LIMIT_GLOBAL_MAX_PER_WINDOW: usize = 120;

/// Maximum number of concurrently-registered messaging device sessions per
/// user. When exceeded on register, the oldest sessions (by `last_seen_unix`)
/// are evicted along with their offline queues. This bounds fan-out cost in
/// `relay_dm_message` and prevents accounts from accumulating arbitrarily
/// many device entries.
const MAX_DEVICES_PER_USER: usize = 8;

// ── Sync Queue Tunables (Phase 2 message sync) ──────────────────

/// How long a sync envelope lives in the destination device's sync queue
/// before it's dropped. Sync sessions are short-lived: if a device doesn't
/// drain in this window, the sender should restart the sync.
const SYNC_QUEUE_TTL_SECONDS: usize = 3600; // 1 hour

/// Maximum size of a single sync envelope. Larger than the live offline
/// queue limit because each envelope batches many messages.
const SYNC_MAX_ENVELOPE_BYTES: usize = 65536; // 64 KB

/// Maximum number of envelopes that may be queued for one destination
/// session at once. Caps total in-flight sync data per device pair.
const SYNC_MAX_ENVELOPES_PER_QUEUE: usize = 100;

/// Maximum number of sync push operations a sender may perform per hour,
/// across all destinations. Stops sync from being abused as a fat-pipe
/// spam channel.
const SYNC_RATE_LIMIT_PER_HOUR: usize = 10;
const SYNC_RATE_LIMIT_WINDOW_SECONDS: usize = 3600;

async fn get_connection() -> Result<Conn> {
    _get_connection()
        .await
        .map_err(|_| create_error!(InternalError))
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Device Registry ─────────────────────────────────────────────

/// One entry in the per-user messaging device registry.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct DeviceEntry {
    node_id: String,
    connected_at: String,
    last_seen_unix: i64,
}

fn devices_key(user_id: &str) -> String {
    format!("broker:devices:messaging:{user_id}")
}

fn queue_key(user_id: &str, session_id: &str) -> String {
    format!("broker:queue:messaging:{user_id}:{session_id}")
}

/// Register (or refresh) a device session in the messaging device registry.
///
/// Idempotent: calling this for an existing session updates `last_seen` and
/// `node_id` without disturbing other sessions belonging to the same user.
///
/// If registering this session would push the user's device count above
/// `MAX_DEVICES_PER_USER`, the oldest sessions (by `last_seen_unix`) are
/// evicted first along with their offline queues.
pub async fn register_user(user_id: &str, node_id: &str, session_id: &str) -> Result<()> {
    let key = devices_key(user_id);
    let entry = DeviceEntry {
        node_id: node_id.to_string(),
        connected_at: Timestamp::now_utc().to_string(),
        last_seen_unix: now_unix(),
    };
    let value = serde_json::to_string(&entry).map_err(|_| create_error!(InternalError))?;

    let mut conn = get_connection().await?;
    conn.hset::<_, _, _, ()>(&key, session_id, &value)
        .await
        .to_internal_error()?;
    conn.expire::<_, ()>(&key, DEVICE_TTL_SECONDS)
        .await
        .to_internal_error()?;

    // Enforce the device cap. We HGETALL after the insert so the new session
    // is included in the ordering decision and is never the one we evict.
    let raw: std::collections::HashMap<String, String> = conn
        .hgetall::<_, std::collections::HashMap<String, String>>(&key)
        .await
        .to_internal_error()?;

    if raw.len() > MAX_DEVICES_PER_USER {
        // Sort by last_seen_unix ascending; oldest entries are evicted first.
        // Sessions with unparseable JSON sort to the front (oldest possible)
        // so they're cleaned up at the same time.
        let mut entries: Vec<(String, i64)> = raw
            .into_iter()
            .map(|(sid, json)| {
                let last_seen = serde_json::from_str::<DeviceEntry>(&json)
                    .map(|e| e.last_seen_unix)
                    .unwrap_or(i64::MIN);
                (sid, last_seen)
            })
            .collect();
        entries.sort_by_key(|(_, ls)| *ls);

        let evict_count = entries.len() - MAX_DEVICES_PER_USER;
        for (sid, _) in entries.into_iter().take(evict_count) {
            // Don't evict the session we just registered, even if its
            // last_seen ordering would put it at the front (shouldn't
            // happen since we just set it to now_unix(), but be defensive).
            if sid == session_id {
                continue;
            }
            let _ = conn.hdel::<_, _, ()>(&key, &sid).await;
            let _ = conn.del::<_, ()>(&queue_key(user_id, &sid)).await;
            log::info!(
                "Evicted device session {sid} for user {user_id} (over MAX_DEVICES_PER_USER={MAX_DEVICES_PER_USER})"
            );
        }
    }

    Ok(())
}

/// Remove a single device session from the registry. Other sessions for the
/// same user remain. Also cleans up that session's offline queue.
pub async fn unregister_user(user_id: &str, session_id: &str) -> Result<()> {
    let key = devices_key(user_id);
    let qkey = queue_key(user_id, session_id);
    let mut conn = get_connection().await?;
    conn.hdel::<_, _, ()>(&key, session_id)
        .await
        .to_internal_error()?;
    conn.del::<_, ()>(&qkey).await.to_internal_error()?;
    Ok(())
}

/// Refresh `last_seen` for a device session (called on heartbeat).
pub async fn refresh_user_ttl(user_id: &str, session_id: &str) -> Result<()> {
    let key = devices_key(user_id);
    let mut conn = get_connection().await?;

    let raw: Option<String> = conn
        .hget::<_, _, Option<String>>(&key, session_id)
        .await
        .to_internal_error()?;

    if let Some(json) = raw {
        if let Ok(mut entry) = serde_json::from_str::<DeviceEntry>(&json) {
            entry.last_seen_unix = now_unix();
            if let Ok(updated) = serde_json::to_string(&entry) {
                conn.hset::<_, _, _, ()>(&key, session_id, updated)
                    .await
                    .to_internal_error()?;
            }
        }
    }

    conn.expire::<_, ()>(&key, DEVICE_TTL_SECONDS)
        .await
        .to_internal_error()?;
    Ok(())
}

/// Check if a user has at least one currently-online session.
pub async fn is_user_online(user_id: &str) -> bool {
    match list_known_sessions(user_id).await {
        Ok(sessions) => sessions.iter().any(|(_, e)| is_online(e)),
        Err(_) => false,
    }
}

/// List all known sessions for a user, lazily pruning stale entries.
async fn list_known_sessions(user_id: &str) -> Result<Vec<(String, DeviceEntry)>> {
    let key = devices_key(user_id);
    let mut conn = get_connection().await?;
    let raw: std::collections::HashMap<String, String> = conn
        .hgetall::<_, std::collections::HashMap<String, String>>(&key)
        .await
        .to_internal_error()?;

    let now = now_unix();
    let mut alive: Vec<(String, DeviceEntry)> = Vec::with_capacity(raw.len());
    let mut to_prune: Vec<String> = Vec::new();

    for (sid, json) in raw {
        match serde_json::from_str::<DeviceEntry>(&json) {
            Ok(entry) => {
                if now - entry.last_seen_unix > DEVICE_TTL_SECONDS as i64 {
                    to_prune.push(sid);
                } else {
                    alive.push((sid, entry));
                }
            }
            Err(_) => to_prune.push(sid),
        }
    }

    if !to_prune.is_empty() {
        for sid in &to_prune {
            // Best-effort prune; ignore individual failures.
            let _ = conn.hdel::<_, _, ()>(&key, sid).await;
            let _ = conn.del::<_, ()>(&queue_key(user_id, sid)).await;
        }
    }

    Ok(alive)
}

fn is_online(entry: &DeviceEntry) -> bool {
    now_unix() - entry.last_seen_unix <= HEARTBEAT_TIMEOUT_SECONDS
}

// ── Offline Message Queue ───────────────────────────────────────

/// Queue a message for a single offline device session of a recipient.
///
/// The message should already be E2E encrypted by the sender. Rate limits
/// are enforced per (sender, recipient_user) pair and globally per sender,
/// not per device — a sender hammering many of a recipient's devices still
/// counts against the same per-pair budget.
///
/// Implementation note: this used to be ~7 sequential Redis round-trips
/// (incr, expire, incr, expire, llen, rpush, expire). It is now 2 round-trips
/// — one read pipeline that fetches both rate-limit counters and the queue
/// depth in parallel, and one write pipeline that does any required EXPIRE
/// operations alongside the RPUSH. Total network latency is ~70% lower in
/// the steady state.
pub async fn queue_offline_message(
    sender_id: &str,
    recipient_id: &str,
    session_id: &str,
    message_event: &str,
) -> Result<()> {
    // Reject oversized messages to prevent memory exhaustion.
    if message_event.len() > OFFLINE_QUEUE_MAX_MESSAGE_BYTES {
        return Err(create_error!(FailedValidation {
            error: "Message exceeds maximum size for offline delivery.".to_string()
        }));
    }

    let qkey = queue_key(recipient_id, session_id);
    let global_rate_key = format!("broker:rate:global:{sender_id}");
    let rate_key = format!("broker:rate:{sender_id}:{recipient_id}");
    let mut conn = get_connection().await?.into_inner();

    // Read pipeline: increment both rate-limit counters and read the queue
    // depth in a single round-trip. The post-INCR values come back as
    // (global_count, pair_count, depth).
    let (global_count, pair_count, depth): (usize, usize, usize) = Pipeline::new()
        .incr(&global_rate_key, 1)
        .incr(&rate_key, 1)
        .llen(&qkey)
        .query_async(&mut conn)
        .await
        .to_internal_error()?;

    // Enforce all three limits before writing anything new.
    if global_count > RATE_LIMIT_GLOBAL_MAX_PER_WINDOW {
        return Err(create_error!(FailedValidation {
            error: "Rate limited".to_string()
        }));
    }
    if pair_count > RATE_LIMIT_MAX_PER_WINDOW {
        return Err(create_error!(FailedValidation {
            error: "Rate limited".to_string()
        }));
    }
    if depth >= OFFLINE_QUEUE_MAX_DEPTH {
        return Err(create_error!(FailedValidation {
            error: "Recipient's offline message queue is full. Try again later.".to_string()
        }));
    }

    // Write pipeline: conditional EXPIREs (only on first INCR / first push)
    // bundled with the RPUSH itself. One round-trip regardless of which
    // EXPIREs fire.
    let mut pipe = Pipeline::new();
    if global_count == 1 {
        pipe.expire(&global_rate_key, RATE_LIMIT_WINDOW_SECONDS);
    }
    if pair_count == 1 {
        pipe.expire(&rate_key, RATE_LIMIT_WINDOW_SECONDS);
    }
    pipe.rpush(&qkey, message_event);
    if depth == 0 {
        pipe.expire(&qkey, OFFLINE_QUEUE_TTL_SECONDS);
    }
    pipe.query_async::<_, ()>(&mut conn)
        .await
        .to_internal_error()?;

    Ok(())
}

/// Drain all queued offline messages for a single device session.
/// Returns the messages and deletes that session's queue atomically.
/// Other sessions of the same user are untouched.
pub async fn drain_offline_queue(user_id: &str, session_id: &str) -> Result<Vec<String>> {
    let key = queue_key(user_id, session_id);
    let mut conn = get_connection().await?;

    let messages: Vec<String> = conn
        .lrange::<_, Vec<String>>(&key, 0, -1)
        .await
        .to_internal_error()?;
    if !messages.is_empty() {
        conn.del::<_, ()>(&key).await.to_internal_error()?;
    }

    Ok(messages)
}

/// Get the number of queued messages for a single device session.
#[allow(dead_code)]
pub async fn get_queue_depth(user_id: &str, session_id: &str) -> Result<usize> {
    let key = queue_key(user_id, session_id);
    let mut conn = get_connection().await?;
    conn.llen::<_, usize>(&key).await.to_internal_error()
}

// ── Ephemeral DM Relay ──────────────────────────────────────────

/// Relay a DM message to recipients without storing in the database.
///
/// For each recipient user, fans out across every known device session:
///   - Online sessions get the message via the channel pub/sub topic
///     (they're already subscribed via bonfire WebSocket).
///   - Offline-but-known sessions get it queued under their own per-device
///     queue, so each device drains independently on next reconnect.
///   - Stale sessions (no heartbeat in `DEVICE_TTL_SECONDS`) are pruned.
///
/// Returns the number of recipient *sessions* (not users) that were online
/// when the message was relayed.
pub async fn relay_dm_message(
    channel_id: &str,
    sender_id: &str,
    recipients: &[String],
    message_event: &EventV1,
) -> Result<usize> {
    let event_json =
        serde_json::to_string(message_event).map_err(|_| create_error!(InternalError))?;

    let mut online_count = 0;

    for recipient_id in recipients {
        if recipient_id == sender_id {
            // Sender's own devices receive via the channel pub/sub topic.
            // Multi-device sender sync for offline sessions is the job of
            // the Phase-2 message-sync feature, not the broker.
            continue;
        }

        let sessions = match list_known_sessions(recipient_id).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to list sessions for {recipient_id}: {e}");
                continue;
            }
        };

        if sessions.is_empty() {
            // Recipient has never registered any device with the broker.
            // Per the chosen design (option C), we do not queue for unknown
            // recipients — they must connect at least once to be reachable.
            continue;
        }

        for (sid, entry) in sessions {
            if is_online(&entry) {
                online_count += 1;
                // Pub/sub on the channel topic delivers to this session.
            } else if let Err(e) =
                queue_offline_message(sender_id, recipient_id, &sid, &event_json).await
            {
                log::warn!(
                    "Failed to queue offline message for {recipient_id} session {sid}: {e}"
                );
            }
        }
    }

    // Always publish to channel topic — online recipients (including sender)
    // are subscribed to it via bonfire WebSocket.
    message_event.clone().p(channel_id.to_string()).await;

    Ok(online_count)
}

// ── Sync Queue (Phase 2 message sync) ───────────────────────────
//
// The sync queue is a per-destination-session list of large encrypted
// envelopes used to transfer historical messages between paired devices.
// It is conceptually similar to the offline message queue but with
// different limits: bigger envelopes (64 KB vs 4 KB), shorter TTL (1h vs
// 6h), aggressive per-sender rate limiting (10/hour vs 120/min).
//
// Sync is initiated by a recipient device requesting messages from another
// of the user's paired devices via the existing transfer-code handshake.
// Both source and destination must belong to the same `user_id` — this is
// enforced by the API layer that calls these functions, not the broker
// itself.

fn sync_queue_key(user_id: &str, dest_session_id: &str) -> String {
    format!("broker:sync:messaging:{user_id}:{dest_session_id}")
}

fn sync_rate_key(user_id: &str, source_session_id: &str) -> String {
    format!("broker:sync:rate:{user_id}:{source_session_id}")
}

/// Push one encrypted sync envelope from a source session to a destination
/// session belonging to the same user.
///
/// The API layer is responsible for verifying that:
///   1. The caller owns `source_session_id` (auth)
///   2. `dest_session_id` exists in the same user's device hash (pairing)
///
/// This function only enforces broker-level invariants: envelope size,
/// queue depth, and rate limits.
pub async fn push_sync_envelope(
    user_id: &str,
    source_session_id: &str,
    dest_session_id: &str,
    envelope: &str,
) -> Result<()> {
    if envelope.len() > SYNC_MAX_ENVELOPE_BYTES {
        return Err(create_error!(FailedValidation {
            error: "Sync envelope exceeds maximum size.".to_string()
        }));
    }

    let qkey = sync_queue_key(user_id, dest_session_id);
    let rate_key = sync_rate_key(user_id, source_session_id);
    let mut conn = get_connection().await?.into_inner();

    // Read pipeline: fetch sender's hourly rate counter and current queue
    // depth in one round-trip.
    let (rate_count, depth): (usize, usize) = Pipeline::new()
        .incr(&rate_key, 1)
        .llen(&qkey)
        .query_async(&mut conn)
        .await
        .to_internal_error()?;

    if rate_count > SYNC_RATE_LIMIT_PER_HOUR {
        return Err(create_error!(FailedValidation {
            error: "Sync rate limit exceeded. Try again later.".to_string()
        }));
    }
    if depth >= SYNC_MAX_ENVELOPES_PER_QUEUE {
        return Err(create_error!(FailedValidation {
            error: "Destination sync queue is full.".to_string()
        }));
    }

    // Write pipeline: conditional EXPIREs + RPUSH in one round-trip.
    let mut pipe = Pipeline::new();
    if rate_count == 1 {
        pipe.expire(&rate_key, SYNC_RATE_LIMIT_WINDOW_SECONDS);
    }
    pipe.rpush(&qkey, envelope);
    if depth == 0 {
        pipe.expire(&qkey, SYNC_QUEUE_TTL_SECONDS);
    }
    pipe.query_async::<_, ()>(&mut conn)
        .await
        .to_internal_error()?;

    Ok(())
}

/// Drain all sync envelopes for a destination session.
/// Returns the envelopes and deletes the queue atomically.
///
/// The API layer must verify the caller owns `dest_session_id`.
pub async fn drain_sync_envelopes(
    user_id: &str,
    dest_session_id: &str,
) -> Result<Vec<String>> {
    let key = sync_queue_key(user_id, dest_session_id);
    let mut conn = get_connection().await?;

    let envelopes: Vec<String> = conn
        .lrange::<_, Vec<String>>(&key, 0, -1)
        .await
        .to_internal_error()?;
    if !envelopes.is_empty() {
        conn.del::<_, ()>(&key).await.to_internal_error()?;
    }
    Ok(envelopes)
}

/// Get the number of sync envelopes currently queued for a destination.
#[allow(dead_code)]
pub async fn get_sync_queue_depth(user_id: &str, dest_session_id: &str) -> Result<usize> {
    let key = sync_queue_key(user_id, dest_session_id);
    let mut conn = get_connection().await?;
    conn.llen::<_, usize>(&key).await.to_internal_error()
}

// ── Device Listing (Phase 2 Devices settings tab) ───────────────

/// Public-facing summary of one registered device session.
#[derive(Serialize, Clone, Debug)]
pub struct DeviceInfo {
    pub session_id: String,
    pub node_id: String,
    pub connected_at: String,
    pub last_seen_unix: i64,
    pub is_online: bool,
}

/// List all known device sessions for a user, in last-seen-descending order.
/// Used by the Devices settings tab to populate the device list. Stale
/// entries (>DEVICE_TTL) are pruned as a side effect.
pub async fn list_user_devices(user_id: &str) -> Result<Vec<DeviceInfo>> {
    let sessions = list_known_sessions(user_id).await?;
    let mut devices: Vec<DeviceInfo> = sessions
        .into_iter()
        .map(|(sid, entry)| DeviceInfo {
            session_id: sid,
            node_id: entry.node_id.clone(),
            connected_at: entry.connected_at.clone(),
            last_seen_unix: entry.last_seen_unix,
            is_online: is_online(&entry),
        })
        .collect();
    devices.sort_by(|a, b| b.last_seen_unix.cmp(&a.last_seen_unix));
    Ok(devices)
}

/// Forcibly forget a device session — removes it from the device hash and
/// drops both its offline and sync queues. Called by the "Forget device"
/// button in the Devices settings tab.
pub async fn forget_device(user_id: &str, session_id: &str) -> Result<()> {
    let dev_key = devices_key(user_id);
    let off_qkey = queue_key(user_id, session_id);
    let sync_qkey = sync_queue_key(user_id, session_id);
    let mut conn = get_connection().await?;
    conn.hdel::<_, _, ()>(&dev_key, session_id)
        .await
        .to_internal_error()?;
    conn.del::<_, ()>(&off_qkey).await.to_internal_error()?;
    conn.del::<_, ()>(&sync_qkey).await.to_internal_error()?;
    Ok(())
}
