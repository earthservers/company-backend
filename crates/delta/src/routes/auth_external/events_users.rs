//! Server-sent stream of `user.updated` events for service-token consumers.
//!
//! Subscribers (EarthSocial backend, etc.) open one stream at boot and keep it
//! alive; on each user-profile mutation in `change_username` / `edit_user` /
//! avatar upload, the corresponding handler calls `publish_user_update` and
//! every connected subscriber receives `{ "type": "user.updated", "id": "<ULID>" }`.
//!
//! The payload is deliberately minimal — consumers refetch via
//! `GET /users/:id` when they care, so the wire format is decoupled from
//! `v0::User`. Heartbeats every 30 s keep proxies from closing idle streams.

use rocket::response::stream::{Event, EventStream};
use rocket::tokio::select;
use rocket::tokio::sync::broadcast::error::RecvError;
use rocket::tokio::time::{interval, Duration};
use rocket::State;

use crate::util::external_auth::ServiceIdentity;
use crate::util::user_events::UserEventBroadcaster;

/// Long-lived SSE stream of `user.updated` events.
///
/// Requires a service token with `users:read` scope. Emits a 30 s heartbeat
/// so reverse proxies don't kill the connection on idle.
#[get("/events/users")]
pub fn user_events(
    service: ServiceIdentity,
    broadcaster: &State<UserEventBroadcaster>,
) -> EventStream![Event] {
    let authorized = service.has_scope("users:read");
    let mut rx = broadcaster.subscribe();
    EventStream! {
        if !authorized {
            yield Event::data(r#"{"error":"insufficient_scope","required":"users:read"}"#)
                .event("error");
            return;
        }

        let mut heartbeat = interval(Duration::from_secs(30));
        // Skip the first immediate tick so the loop's first iteration waits.
        heartbeat.tick().await;

        loop {
            select! {
                tick = heartbeat.tick() => {
                    let _ = tick;
                    yield Event::data("{}").event("heartbeat");
                }
                msg = rx.recv() => {
                    match msg {
                        Ok(event) => {
                            // Serialise to JSON; fall back to a sentinel if it fails (shouldn't,
                            // event is a tiny struct with two string fields).
                            let payload = serde_json::to_string(&event)
                                .unwrap_or_else(|_| r#"{"type":"user.updated","id":""}"#.to_owned());
                            yield Event::data(payload);
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            log::warn!(
                                "user_events SSE subscriber lagged, skipped {skipped} events; \
                                 subscriber should fall back to /users/:id refetch"
                            );
                            yield Event::data(r#"{"type":"lagged"}"#).event("warning");
                        }
                        Err(RecvError::Closed) => {
                            // Broadcaster dropped — shouldn't happen since it's managed state
                            // for the lifetime of the Rocket app. End the stream cleanly.
                            return;
                        }
                    }
                }
            }
        }
    }
}
