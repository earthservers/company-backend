use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

/// Metadata for a pending interaction (slash command invocation)
#[derive(Debug, Clone)]
pub struct PendingInteraction {
    /// Channel where the command was invoked
    pub channel_id: String,
    /// User who invoked the command
    pub user_id: String,
    /// Bot that should handle the interaction
    pub bot_id: String,
    /// Command name
    pub command_name: String,
    /// Auth token for the callback
    pub token: String,
    /// When the interaction was created
    pub created_at: Instant,
}

/// In-memory store for pending interactions
/// Interactions expire after 15 minutes
#[derive(Debug, Clone)]
pub struct InteractionStore {
    inner: Arc<DashMap<String, PendingInteraction>>,
}

impl InteractionStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Insert a pending interaction
    pub fn insert(&self, id: String, interaction: PendingInteraction) {
        self.inner.insert(id, interaction);
    }

    /// Look up and remove a pending interaction
    pub fn take(&self, id: &str) -> Option<PendingInteraction> {
        self.inner.remove(id).map(|(_, v)| v)
    }

    /// Look up a pending interaction without removing it
    pub fn get(&self, id: &str) -> Option<PendingInteraction> {
        self.inner.get(id).map(|v| v.clone())
    }

    /// Remove expired interactions (older than 15 minutes)
    pub fn cleanup_expired(&self) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(15 * 60);
        self.inner.retain(|_, v| v.created_at > cutoff);
    }
}
