use company_models::v0::{
    CurrentStreamMetrics, StreamSession, StreamSessionSummary, ViewerSnapshot,
};
use company_result::{create_error, Result, ToRevoltError};
use redis_kiss::{get_connection as _get_connection, AsyncCommands, Conn};

use crate::Database;

/// MongoDB collection name for stream sessions
const STREAM_SESSIONS_COL: &str = "stream_sessions";

async fn get_connection() -> Result<Conn> {
    _get_connection()
        .await
        .map_err(|_| create_error!(InternalError))
}

/// Create a new stream session when the first person joins a voice channel.
pub async fn create_stream_session(
    db: &Database,
    session_id: &str,
    channel_id: &str,
    streamer_id: &str,
    server_id: Option<&str>,
) -> Result<()> {
    let now = now_ms();

    let session = StreamSession {
        id: session_id.to_string(),
        channel_id: channel_id.to_string(),
        streamer_id: streamer_id.to_string(),
        server_id: server_id.map(String::from),
        started_at: now,
        ended_at: None,
        current_viewers: 0,
        current_participants: 0,
        peak_viewers: 0,
        peak_participants: 0,
        duration_seconds: 0,
        average_viewers: 0.0,
    };

    #[cfg(feature = "mongodb")]
    {
        let mongo = db.mongodb();
        mongo
            .col::<StreamSession>(STREAM_SESSIONS_COL)
            .insert_one(&session)
            .await
            .map_err(|_| create_database_error!("insert_one", STREAM_SESSIONS_COL))?;
    }

    Ok(())
}

/// Update viewer and participant counts for an active session.
/// Uses aggregation pipeline update for $max on peak counts.
pub async fn update_stream_counts(
    db: &Database,
    session_id: &str,
    viewers: u32,
    participants: u32,
) -> Result<()> {
    #[cfg(feature = "mongodb")]
    {
        use mongodb::bson::doc;

        let mongo = db.mongodb();
        mongo
            .col::<StreamSession>(STREAM_SESSIONS_COL)
            .update_one(
                doc! { "_id": session_id },
                vec![doc! {
                    "$set": {
                        "current_viewers": viewers as i64,
                        "current_participants": participants as i64,
                        "peak_viewers": {
                            "$max": ["$peak_viewers", viewers as i64]
                        },
                        "peak_participants": {
                            "$max": ["$peak_participants", participants as i64]
                        },
                    }
                }],
            )
            .await
            .map_err(|_| create_database_error!("update_one", STREAM_SESSIONS_COL))?;
    }

    Ok(())
}

/// End a stream session: set ended_at, calculate duration and average viewers.
pub async fn end_stream_session(db: &Database, session_id: &str) -> Result<()> {
    #[cfg(feature = "mongodb")]
    {
        use mongodb::bson::doc;

        let mongo = db.mongodb();
        let now = now_ms();

        // Fetch session to calculate duration
        let session: Option<StreamSession> = mongo
            .col::<StreamSession>(STREAM_SESSIONS_COL)
            .find_one(doc! { "_id": session_id })
            .await
            .map_err(|_| create_database_error!("find_one", STREAM_SESSIONS_COL))?;

        if let Some(session) = session {
            let duration_seconds = ((now - session.started_at) / 1000).max(0) as u64;

            // Get snapshots from Redis to calculate average
            let snapshots = get_viewer_snapshots(session_id).await;
            let average_viewers = if snapshots.is_empty() {
                0.0
            } else {
                let total: u64 = snapshots.iter().map(|s| s.viewers as u64).sum();
                total as f64 / snapshots.len() as f64
            };

            mongo
                .col::<StreamSession>(STREAM_SESSIONS_COL)
                .update_one(
                    doc! { "_id": session_id },
                    doc! {
                        "$set": {
                            "ended_at": now,
                            "duration_seconds": duration_seconds as i64,
                            "average_viewers": average_viewers,
                            "current_viewers": 0_i32,
                            "current_participants": 0_i32,
                        }
                    },
                )
                .await
                .map_err(|_| create_database_error!("update_one", STREAM_SESSIONS_COL))?;
        }
    }

    Ok(())
}

/// Get the active (not ended) stream session for a channel from MongoDB.
pub async fn get_active_session(
    db: &Database,
    channel_id: &str,
) -> Result<Option<StreamSession>> {
    #[cfg(feature = "mongodb")]
    {
        use mongodb::bson::doc;

        let mongo = db.mongodb();
        let session = mongo
            .col::<StreamSession>(STREAM_SESSIONS_COL)
            .find_one(doc! {
                "channel_id": channel_id,
                "ended_at": null,
            })
            .await
            .map_err(|_| create_database_error!("find_one", STREAM_SESSIONS_COL))?;

        return Ok(session);
    }

    #[cfg(not(feature = "mongodb"))]
    Ok(None)
}

/// Get current stream metrics for the analytics API endpoint.
pub async fn get_current_metrics(
    db: &Database,
    channel_id: &str,
) -> Result<CurrentStreamMetrics> {
    let session = get_active_session(db, channel_id).await?;

    match session {
        Some(session) => {
            let now = now_ms();
            let duration_seconds = ((now - session.started_at) / 1000).max(0) as u64;
            let viewer_history = get_viewer_snapshots(&session.id).await;

            Ok(CurrentStreamMetrics {
                is_live: true,
                session_id: Some(session.id),
                current_viewers: session.current_viewers,
                peak_viewers: session.peak_viewers,
                started_at: Some(session.started_at),
                duration_seconds,
                viewer_history,
            })
        }
        None => Ok(CurrentStreamMetrics {
            is_live: false,
            session_id: None,
            current_viewers: 0,
            peak_viewers: 0,
            started_at: None,
            duration_seconds: 0,
            viewer_history: Vec::new(),
        }),
    }
}

/// Get historical stream session summaries for a channel.
pub async fn get_stream_history(
    db: &Database,
    channel_id: &str,
    days: u32,
) -> Result<Vec<StreamSessionSummary>> {
    #[cfg(feature = "mongodb")]
    {
        use futures::StreamExt;
        use mongodb::bson::doc;

        let mongo = db.mongodb();
        let cutoff_ms = now_ms() - (days as i64 * 24 * 60 * 60 * 1000);

        let mut cursor = mongo
            .col::<StreamSession>(STREAM_SESSIONS_COL)
            .find(doc! {
                "channel_id": channel_id,
                "ended_at": { "$ne": null },
                "started_at": { "$gte": cutoff_ms },
            })
            .sort(doc! { "started_at": -1 })
            .await
            .map_err(|_| create_database_error!("find", STREAM_SESSIONS_COL))?;

        let mut summaries = Vec::new();
        while let Some(Ok(session)) = cursor.next().await {
            summaries.push(StreamSessionSummary {
                id: session.id,
                started_at: session.started_at,
                duration_seconds: session.duration_seconds,
                peak_viewers: session.peak_viewers,
                average_viewers: session.average_viewers,
            });
        }

        return Ok(summaries);
    }

    #[cfg(not(feature = "mongodb"))]
    Ok(Vec::new())
}

// --- Redis helpers ---

/// Get the active session ID from Redis cache.
pub async fn get_cached_session_id(channel_id: &str) -> Option<String> {
    get_connection()
        .await
        .ok()?
        .get(format!("stream_session:{channel_id}"))
        .await
        .ok()
}

/// Cache a session ID in Redis with 24h TTL.
pub async fn cache_session_id(channel_id: &str, session_id: &str) {
    if let Ok(mut conn) = get_connection().await {
        let _: std::result::Result<(), _> = conn
            .set_ex(format!("stream_session:{channel_id}"), session_id, 86400)
            .await;
    }
}

/// Remove cached session ID from Redis.
pub async fn clear_cached_session(channel_id: &str) {
    if let Ok(mut conn) = get_connection().await {
        let _: std::result::Result<(), _> =
            conn.del(format!("stream_session:{channel_id}")).await;
    }
}

/// Record a viewer/participant count snapshot in Redis sorted set.
pub async fn record_snapshot(session_id: &str, viewers: u32, participants: u32) {
    if let Ok(mut conn) = get_connection().await {
        let now = now_ms() / 1000; // seconds
        let value = format!("{viewers}:{participants}");
        let _: std::result::Result<(), _> = conn
            .zadd(format!("stream_snapshots:{session_id}"), &value, now)
            .await;
    }
}

/// Get viewer snapshots from Redis for the last 2 hours.
pub async fn get_viewer_snapshots(session_id: &str) -> Vec<ViewerSnapshot> {
    let Ok(mut conn) = get_connection().await else {
        return Vec::new();
    };

    let now = now_ms() / 1000;
    let two_hours_ago = now - 7200;

    let results: Vec<(String, f64)> = conn
        .zrangebyscore_withscores(
            format!("stream_snapshots:{session_id}"),
            two_hours_ago,
            now,
        )
        .await
        .unwrap_or_default();

    results
        .into_iter()
        .filter_map(|(value, score)| {
            let parts: Vec<&str> = value.split(':').collect();
            if parts.len() == 2 {
                Some(ViewerSnapshot {
                    timestamp: score as i64,
                    viewers: parts[0].parse().unwrap_or(0),
                    participants: parts[1].parse().unwrap_or(0),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Clean up Redis keys for a session. Keeps snapshots with 7-day TTL.
pub async fn cleanup_redis(channel_id: &str, session_id: &str) {
    if let Ok(mut conn) = get_connection().await {
        let _: std::result::Result<(), _> =
            conn.del(format!("stream_session:{channel_id}")).await;
        let _: std::result::Result<(), _> = conn
            .expire(format!("stream_snapshots:{session_id}"), 604800)
            .await;
    }
}

/// Get current Unix timestamp in milliseconds.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
