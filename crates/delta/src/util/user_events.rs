//! In-process broadcast channel for user-update events.
//!
//! Publishers (`change_username`, `edit_user`, future avatar uploads) call
//! [`publish_user_update`] after a successful mutation. Subscribers (the
//! `/auth/events/users` SSE endpoint) receive every event and forward it to
//! external consumers (currently EarthSocial; future Earth Servers services).
//!
//! ## Payload shape
//!
//! Deliberately minimal: `{ "type": "user.updated", "id": "<ULID>" }`. The
//! event is a *poke* — consumers refetch via `GET /users/:id` if they care.
//! This keeps the wire format decoupled from the v0::User shape so we can
//! evolve the user model without breaking subscribers.
//!
//! ## Scope and limitations
//!
//! Single-instance only. If Company runs behind a load balancer with multiple
//! Rocket instances, each instance has its own broadcaster and subscribers
//! only see events that originate on the same instance. Cross-instance fanout
//! requires routing publishes through RabbitMQ (`AMQP` is already available
//! as managed state); the broadcaster API stays the same.
//!
//! Capacity: 1024 events buffered per subscriber. Slow consumers that fall
//! behind are dropped (lagged) — `broadcast::Receiver::recv` returns
//! `RecvError::Lagged` and the SSE handler logs + continues. EarthSocial's
//! 1-hour TTL refresh covers gaps.

use rocket::tokio::sync::broadcast;
use rocket::State;
use serde::Serialize;

/// Channel capacity. Generous: each event is ~80 bytes; 1024 buffered events
/// is ~80 KB per subscriber. A subscriber would have to be ~30 seconds behind
/// at our expected event rate to lag.
const CHANNEL_CAPACITY: usize = 1024;

/// One user-mutation event. Sent as the `data:` payload of an SSE `Event`.
#[derive(Debug, Clone, Serialize)]
pub struct UserUpdatedEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub id: String,
}

impl UserUpdatedEvent {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            event_type: "user.updated",
            id: user_id.into(),
        }
    }
}

/// Newtype around the broadcast sender so Rocket's managed-state lookup is
/// unambiguous. Held as `State<UserEventBroadcaster>` by both publishers and
/// the SSE endpoint.
#[derive(Clone)]
pub struct UserEventBroadcaster {
    tx: broadcast::Sender<UserUpdatedEvent>,
}

impl Default for UserEventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl UserEventBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UserUpdatedEvent> {
        self.tx.subscribe()
    }

    /// Send an event to all current subscribers. Returns the number of
    /// subscribers that received it (0 if no one is listening — that's fine,
    /// not an error).
    pub fn publish(&self, event: UserUpdatedEvent) {
        // `send` returns Err only when there are no receivers; we don't care.
        let _ = self.tx.send(event);
    }
}

/// Convenience helper for route handlers. Publishes a `user.updated` event
/// for the given user id. Non-failing — broadcasting is best-effort and
/// mustn't break the underlying mutation if no one is listening.
pub fn publish_user_update(state: &State<UserEventBroadcaster>, user_id: &str) {
    state.publish(UserUpdatedEvent::new(user_id));
}
