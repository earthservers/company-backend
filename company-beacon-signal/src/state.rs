use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc::UnboundedSender, RwLock};

use axum::extract::ws::Message;

pub const MAX_PEERS: usize = 10_000;
pub const MAX_SESSIONS: usize = 1_000;
pub const SESSION_TTL_SECS: u64 = 60;

#[allow(dead_code)]
pub type SharedState = Arc<RwLock<SignalingState>>;

pub struct PeerEntry {
    pub peer_id: String,
    pub file_hashes: Vec<String>,
    pub multiaddr: Option<String>,
    pub tx: UnboundedSender<Message>,
    #[allow(dead_code)]
    pub connected_at: Instant,
}

#[derive(Default)]
pub struct SignalingState {
    pub peers: HashMap<String, PeerEntry>,
    pub hash_index: HashMap<String, String>,
}

impl SignalingState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or re-register a peer. Returns Err if at capacity and peer is new.
    pub fn register(&mut self, entry: PeerEntry) -> Result<(), ()> {
        let is_existing = self.peers.contains_key(&entry.peer_id);
        if !is_existing && self.peers.len() >= MAX_PEERS {
            return Err(());
        }

        // If re-registering, clean up old hashes first
        if is_existing {
            self.remove_hashes(&entry.peer_id);
        }

        // Index new hashes — first registrant wins
        for hash in &entry.file_hashes {
            self.hash_index
                .entry(hash.clone())
                .or_insert_with(|| entry.peer_id.clone());
        }

        self.peers.insert(entry.peer_id.clone(), entry);
        Ok(())
    }

    /// Remove a peer entirely. Returns the hashes that were removed from the index.
    pub fn remove(&mut self, peer_id: &str) -> Vec<String> {
        let removed_hashes = self.remove_hashes(peer_id);
        self.peers.remove(peer_id);
        removed_hashes
    }

    /// Resolve a hash to its owning peer.
    pub fn resolve(&self, hash: &str) -> Option<&PeerEntry> {
        let peer_id = self.hash_index.get(hash)?;
        self.peers.get(peer_id)
    }

    /// Get a peer by ID.
    pub fn get_peer(&self, peer_id: &str) -> Option<&PeerEntry> {
        self.peers.get(peer_id)
    }

    /// Remove all hashes owned by a peer from the index.
    fn remove_hashes(&mut self, peer_id: &str) -> Vec<String> {
        let mut removed = Vec::new();
        self.hash_index.retain(|hash, owner| {
            if owner == peer_id {
                removed.push(hash.clone());
                false
            } else {
                true
            }
        });
        removed
    }
}

// ─── Session token store ───

pub struct SessionEntry {
    pub peer_id: String,
    pub created_at: Instant,
    pub used: bool,
}

#[derive(Default)]
pub struct SessionStore {
    pub tokens: HashMap<String, SessionEntry>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a session token. Evicts expired entries first.
    /// Returns false if at capacity after eviction.
    pub fn insert(&mut self, token: String, peer_id: String) -> bool {
        self.evict_expired();
        if self.tokens.len() >= MAX_SESSIONS {
            return false;
        }
        self.tokens.insert(
            token,
            SessionEntry {
                peer_id,
                created_at: Instant::now(),
                used: false,
            },
        );
        true
    }

    /// Consume a session token. Returns the peer_id if valid, not expired, not already used.
    /// Marks the token as used immediately — single use.
    pub fn consume(&mut self, token: &str) -> Option<String> {
        let entry = self.tokens.get_mut(token)?;
        if entry.used {
            return None;
        }
        if entry.created_at.elapsed().as_secs() >= SESSION_TTL_SECS {
            return None;
        }
        entry.used = true;
        Some(entry.peer_id.clone())
    }

    /// Remove all entries older than SESSION_TTL_SECS.
    pub fn evict_expired(&mut self) {
        self.tokens
            .retain(|_, entry| entry.created_at.elapsed().as_secs() < SESSION_TTL_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn make_entry(peer_id: &str, hashes: &[&str]) -> PeerEntry {
        let (tx, _rx) = mpsc::unbounded_channel();
        PeerEntry {
            peer_id: peer_id.to_string(),
            file_hashes: hashes.iter().map(|s| s.to_string()).collect(),
            multiaddr: Some(format!("/ip4/127.0.0.1/tcp/9000/p2p/{peer_id}")),
            tx,
            connected_at: Instant::now(),
        }
    }

    // ─── SignalingState tests ───

    #[test]
    fn register_and_resolve() {
        let mut state = SignalingState::new();
        state.register(make_entry("peer-a", &["hash1", "hash2"])).unwrap();

        let entry = state.resolve("hash1").unwrap();
        assert_eq!(entry.peer_id, "peer-a");

        let entry = state.resolve("hash2").unwrap();
        assert_eq!(entry.peer_id, "peer-a");

        assert!(state.resolve("hash3").is_none());
    }

    #[test]
    fn first_registrant_wins() {
        let mut state = SignalingState::new();
        state.register(make_entry("peer-a", &["hash1"])).unwrap();
        state.register(make_entry("peer-b", &["hash1"])).unwrap();

        let entry = state.resolve("hash1").unwrap();
        assert_eq!(entry.peer_id, "peer-a");
    }

    #[test]
    fn re_register_updates_hashes() {
        let mut state = SignalingState::new();
        state.register(make_entry("peer-a", &["hash1"])).unwrap();
        state.register(make_entry("peer-a", &["hash2"])).unwrap();

        assert!(state.resolve("hash1").is_none());
        let entry = state.resolve("hash2").unwrap();
        assert_eq!(entry.peer_id, "peer-a");
    }

    #[test]
    fn remove_cleans_hashes() {
        let mut state = SignalingState::new();
        state.register(make_entry("peer-a", &["hash1", "hash2"])).unwrap();

        let removed = state.remove("peer-a");
        assert_eq!(removed.len(), 2);
        assert!(state.resolve("hash1").is_none());
        assert!(state.resolve("hash2").is_none());
        assert!(state.get_peer("peer-a").is_none());
    }

    #[test]
    fn remove_frees_hash_for_new_registrant() {
        let mut state = SignalingState::new();
        state.register(make_entry("peer-a", &["hash1"])).unwrap();
        state.remove("peer-a");
        state.register(make_entry("peer-b", &["hash1"])).unwrap();

        let entry = state.resolve("hash1").unwrap();
        assert_eq!(entry.peer_id, "peer-b");
    }

    #[test]
    fn max_peers_enforced() {
        let mut state = SignalingState::new();
        for i in 0..MAX_PEERS {
            state.register(make_entry(&format!("peer-{i}"), &[])).unwrap();
        }
        let result = state.register(make_entry("peer-overflow", &[]));
        assert!(result.is_err());
    }

    #[test]
    fn re_register_at_capacity_succeeds() {
        let mut state = SignalingState::new();
        for i in 0..MAX_PEERS {
            state.register(make_entry(&format!("peer-{i}"), &[])).unwrap();
        }
        let result = state.register(make_entry("peer-0", &["newhash"]));
        assert!(result.is_ok());
    }

    // ─── SessionStore tests ───

    #[test]
    fn session_insert_and_consume() {
        let mut store = SessionStore::new();
        assert!(store.insert("tok-1".into(), "peer-a".into()));
        let peer = store.consume("tok-1").unwrap();
        assert_eq!(peer, "peer-a");
    }

    #[test]
    fn session_single_use() {
        let mut store = SessionStore::new();
        store.insert("tok-1".into(), "peer-a".into());
        assert!(store.consume("tok-1").is_some());
        assert!(store.consume("tok-1").is_none());
    }

    #[test]
    fn session_missing_token() {
        let mut store = SessionStore::new();
        assert!(store.consume("nonexistent").is_none());
    }

    #[test]
    fn session_expired_token() {
        let mut store = SessionStore::new();
        store.tokens.insert(
            "old-tok".into(),
            SessionEntry {
                peer_id: "peer-a".into(),
                created_at: Instant::now() - std::time::Duration::from_secs(SESSION_TTL_SECS + 1),
                used: false,
            },
        );
        assert!(store.consume("old-tok").is_none());
    }

    #[test]
    fn session_capacity_enforced() {
        let mut store = SessionStore::new();
        for i in 0..MAX_SESSIONS {
            assert!(store.insert(format!("tok-{i}"), "peer".into()));
        }
        assert!(!store.insert("overflow".into(), "peer".into()));
    }

    #[test]
    fn session_eviction_frees_capacity() {
        let mut store = SessionStore::new();
        // Fill with expired entries
        for i in 0..MAX_SESSIONS {
            store.tokens.insert(
                format!("tok-{i}"),
                SessionEntry {
                    peer_id: "peer".into(),
                    created_at: Instant::now() - std::time::Duration::from_secs(SESSION_TTL_SECS + 1),
                    used: false,
                },
            );
        }
        // Insert should succeed after eviction
        assert!(store.insert("new-tok".into(), "peer-new".into()));
        assert_eq!(store.tokens.len(), 1);
    }
}
