use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Json, State};
use axum::response::IntoResponse;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use constant_time_eq::constant_time_eq;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use http::StatusCode;
use serde::Deserialize;
use sha2::Sha256;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use crate::messages::{AuthError, AuthRequest, AuthResponse, InboundMessage, OutboundMessage};
use crate::state::{PeerEntry, SignalingState};
use crate::AppState;

// ─── Layer 1: HMAC client authentication ───

/// Verify the HMAC sent on WebSocket upgrade headers.
/// Returns true if the HMAC is valid and the timestamp is within 30 seconds.
pub fn verify_client_hmac(
    peer_id: &str,
    timestamp_str: &str,
    hmac_header: &str,
    secret: &[u8],
) -> bool {
    // 1. Parse timestamp, reject if > 30 seconds old
    let timestamp: u64 = match timestamp_str.parse() {
        Ok(t) => t,
        Err(_) => return false,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now.abs_diff(timestamp) > 30 {
        return false;
    }

    // 2. Compute expected HMAC
    let message = format!("{}:{}", peer_id, timestamp_str);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(message.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    // 3. Constant-time compare
    constant_time_eq(hmac_header.as_bytes(), expected.as_bytes())
}

// ─── Auth endpoint handler ───

pub async fn auth_handler(
    State(app): State<AppState>,
    Json(body): Json<AuthRequest>,
) -> impl IntoResponse {
    // 1. If SIGNAL_SECRET is set, verify HMAC
    if let Some(secret) = &app.signal_secret {
        if !verify_client_hmac(&body.peer_id, &body.timestamp, &body.hmac, secret) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::to_value(AuthError {
                    code: "AUTH_FAILED".into(),
                })
                .unwrap()),
            )
                .into_response();
        }
    }

    // 2. Generate session token
    let token = uuid::Uuid::new_v4().to_string();

    // 3. Insert into session store
    let mut sessions = app.sessions.write().await;
    if !sessions.insert(token.clone(), body.peer_id) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                serde_json::to_value(AuthError {
                    code: "CAPACITY".into(),
                })
                .unwrap(),
            ),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(serde_json::to_value(AuthResponse { session_token: token }).unwrap()),
    )
        .into_response()
}

// ─── Rate limiter ───

/// Rate limiter: token bucket, max 10 messages/sec per connection.
struct RateLimiter {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            tokens: 10.0,
            max_tokens: 10.0,
            refill_rate: 10.0,
            last_refill: Instant::now(),
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

// ─── Layer 2: Ed25519 JWT validation ───

/// JWT header with embedded public key.
#[derive(Debug, Deserialize)]
struct JwtHeader {
    alg: String,
    pubkey: String,
}

/// JWT claims we care about.
#[derive(Debug, Deserialize)]
struct JwtClaims {
    sub: Option<String>,
    exp: Option<u64>,
}

/// Validate an Ed25519 self-signed JWT. Returns the `sub` claim (peer ID) on success.
fn validate_token(token: &str) -> Result<String, &'static str> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("AUTH_FAILED");
    }

    // Decode header
    let header_bytes = URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|_| "AUTH_FAILED")?;
    let header: JwtHeader =
        serde_json::from_slice(&header_bytes).map_err(|_| "AUTH_FAILED")?;

    if header.alg != "EdDSA" {
        return Err("AUTH_FAILED");
    }

    // Decode public key from header
    let pubkey_bytes = URL_SAFE_NO_PAD
        .decode(&header.pubkey)
        .map_err(|_| "AUTH_FAILED")?;
    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| "AUTH_FAILED")?;
    let verifying_key =
        VerifyingKey::from_bytes(&pubkey_array).map_err(|_| "AUTH_FAILED")?;

    // Verify signature over "header.payload"
    let signed_content = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| "AUTH_FAILED")?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "AUTH_FAILED")?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify(signed_content.as_bytes(), &signature)
        .map_err(|_| "AUTH_FAILED")?;

    // Decode claims
    let claims_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| "AUTH_FAILED")?;
    let claims: JwtClaims =
        serde_json::from_slice(&claims_bytes).map_err(|_| "AUTH_FAILED")?;

    // Check expiry
    if let Some(exp) = claims.exp {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now > exp {
            return Err("AUTH_FAILED");
        }
    }

    claims.sub.ok_or("AUTH_FAILED")
}

// ─── WebSocket message handling ───

fn send_msg(msg: &OutboundMessage) -> Message {
    Message::Text(serde_json::to_string(msg).unwrap().into())
}

pub async fn handle_socket(socket: WebSocket, state: Arc<RwLock<SignalingState>>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Spawn task to forward channel messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut peer_id: Option<String> = None;
    let mut rate_limiter = RateLimiter::new();

    // Log connection
    {
        let s = state.read().await;
        info!(connections = s.peers.len() + 1, "peer connected");
    }

    // Process incoming messages
    while let Some(Ok(msg)) = ws_receiver.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        if !rate_limiter.allow() {
            let _ = tx.send(send_msg(&OutboundMessage::error("RATE_LIMITED")));
            continue;
        }

        let inbound: InboundMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => {
                let _ = tx.send(send_msg(&OutboundMessage::error("MALFORMED")));
                continue;
            }
        };

        match inbound {
            InboundMessage::Register {
                peer_id: _client_peer_id,
                file_hashes,
                multiaddr,
                token,
            } => {
                let authenticated_peer_id = match validate_token(&token) {
                    Ok(sub) => sub,
                    Err(code) => {
                        let _ = tx.send(send_msg(&OutboundMessage::error(code)));
                        continue;
                    }
                };

                let entry = PeerEntry {
                    peer_id: authenticated_peer_id.clone(),
                    file_hashes,
                    multiaddr,
                    tx: tx.clone(),
                    connected_at: Instant::now(),
                };

                let mut s = state.write().await;
                if s.register(entry).is_err() {
                    let _ = tx.send(send_msg(&OutboundMessage::error("CAPACITY_EXCEEDED")));
                    continue;
                }

                peer_id = Some(authenticated_peer_id.clone());
                let _ = tx.send(send_msg(&OutboundMessage::Registered {
                    peer_id: authenticated_peer_id,
                }));
            }

            InboundMessage::Resolve { hash, token } => {
                let _sub = match validate_token(&token) {
                    Ok(sub) => sub,
                    Err(code) => {
                        let _ = tx.send(send_msg(&OutboundMessage::error(code)));
                        continue;
                    }
                };

                let s = state.read().await;
                match s.resolve(&hash) {
                    Some(entry) => {
                        let _ = tx.send(send_msg(&OutboundMessage::Resolved {
                            hash,
                            peer_id: entry.peer_id.clone(),
                            multiaddr: entry.multiaddr.clone(),
                        }));
                    }
                    None => {
                        let _ = tx.send(send_msg(&OutboundMessage::NotFound { hash }));
                    }
                }
            }

            InboundMessage::Signal {
                to_peer_id,
                payload,
                token,
            } => {
                let from_peer_id = match validate_token(&token) {
                    Ok(sub) => sub,
                    Err(code) => {
                        let _ = tx.send(send_msg(&OutboundMessage::error(code)));
                        continue;
                    }
                };

                let s = state.read().await;
                match s.get_peer(&to_peer_id) {
                    Some(target) => {
                        let _ = target.tx.send(send_msg(&OutboundMessage::Signal {
                            from_peer_id,
                            payload,
                        }));
                    }
                    None => {
                        let _ =
                            tx.send(send_msg(&OutboundMessage::error("PEER_NOT_FOUND")));
                    }
                }
            }

            InboundMessage::Leave => break,
        }
    }

    // Disconnect cleanup
    if let Some(pid) = &peer_id {
        let mut s = state.write().await;
        let _removed_hashes = s.remove(pid);

        // Best-effort broadcast PeerLeft to all connected peers
        let peer_left_msg = send_msg(&OutboundMessage::PeerLeft {
            peer_id: pid.clone(),
        });
        for entry in s.peers.values() {
            let _ = entry.tx.send(peer_left_msg.clone());
        }

        info!(connections = s.peers.len(), "peer disconnected");
    }

    send_task.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SessionStore, SESSION_TTL_SECS};
    use ed25519_dalek::SigningKey;

    /// Create a valid self-signed JWT for testing.
    fn make_test_token(peer_id: &str, expired: bool) -> String {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        make_test_token_with_key(peer_id, &signing_key, expired)
    }

    fn make_test_token_with_key(
        peer_id: &str,
        signing_key: &SigningKey,
        expired: bool,
    ) -> String {
        let pubkey_b64 = URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes());

        let header = serde_json::json!({ "alg": "EdDSA", "pubkey": pubkey_b64 });
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());

        let exp = if expired { 0u64 } else { u64::MAX };
        let claims = serde_json::json!({ "sub": peer_id, "exp": exp });
        let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());

        let signed_content = format!("{header_b64}.{claims_b64}");
        let signature: Signature = signing_key.sign(signed_content.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        format!("{header_b64}.{claims_b64}.{sig_b64}")
    }

    // ─── Layer 2: JWT tests ───

    #[test]
    fn token_validation_valid() {
        let token = make_test_token("peer-a", false);
        let sub = validate_token(&token).unwrap();
        assert_eq!(sub, "peer-a");
    }

    #[test]
    fn token_validation_expired() {
        let token = make_test_token("peer-a", true);
        assert_eq!(validate_token(&token), Err("AUTH_FAILED"));
    }

    #[test]
    fn token_validation_tampered() {
        let token = make_test_token("peer-a", false);
        let parts: Vec<&str> = token.split('.').collect();
        let bad_claims = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({"sub": "evil", "exp": u64::MAX})).unwrap(),
        );
        let tampered = format!("{}.{}.{}", parts[0], bad_claims, parts[2]);
        assert_eq!(validate_token(&tampered), Err("AUTH_FAILED"));
    }

    #[test]
    fn token_validation_bad_format() {
        assert_eq!(validate_token("not.a.jwt.at.all.really"), Err("AUTH_FAILED"));
        assert_eq!(validate_token("garbage"), Err("AUTH_FAILED"));
    }

    // ─── Layer 1: HMAC tests ───

    fn compute_hmac(peer_id: &str, timestamp: u64, secret: &[u8]) -> String {
        let message = format!("{}:{}", peer_id, timestamp);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(message.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn hmac_valid() {
        let secret = b"test-secret-key";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hmac = compute_hmac("peer-a", now, secret);
        assert!(verify_client_hmac("peer-a", &now.to_string(), &hmac, secret));
    }

    #[test]
    fn hmac_wrong_secret() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hmac = compute_hmac("peer-a", now, b"correct-secret");
        assert!(!verify_client_hmac(
            "peer-a",
            &now.to_string(),
            &hmac,
            b"wrong-secret"
        ));
    }

    #[test]
    fn hmac_wrong_peer_id() {
        let secret = b"test-secret-key";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hmac = compute_hmac("peer-a", now, secret);
        assert!(!verify_client_hmac(
            "peer-b",
            &now.to_string(),
            &hmac,
            secret
        ));
    }

    #[test]
    fn hmac_expired_timestamp() {
        let secret = b"test-secret-key";
        let old = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 60;
        let hmac = compute_hmac("peer-a", old, secret);
        assert!(!verify_client_hmac(
            "peer-a",
            &old.to_string(),
            &hmac,
            secret
        ));
    }

    #[test]
    fn hmac_future_timestamp_within_window() {
        let secret = b"test-secret-key";
        let future = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 15;
        let hmac = compute_hmac("peer-a", future, secret);
        assert!(verify_client_hmac(
            "peer-a",
            &future.to_string(),
            &hmac,
            secret
        ));
    }

    #[test]
    fn hmac_invalid_timestamp() {
        let secret = b"test-secret-key";
        assert!(!verify_client_hmac("peer-a", "not-a-number", "whatever", secret));
    }

    #[test]
    fn hmac_tampered_value() {
        let secret = b"test-secret-key";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hmac = compute_hmac("peer-a", now, secret);
        // Flip a character
        let mut tampered: Vec<u8> = hmac.into_bytes();
        tampered[0] ^= 0x01;
        let tampered = String::from_utf8(tampered).unwrap();
        assert!(!verify_client_hmac(
            "peer-a",
            &now.to_string(),
            &tampered,
            secret
        ));
    }

    // ─── Auth endpoint tests ───

    #[tokio::test]
    async fn auth_valid_request_returns_session_token() {
        let secret = b"test-secret".to_vec();
        let app = AppState {
            signaling: Arc::new(RwLock::new(SignalingState::new())),
            sessions: Arc::new(RwLock::new(SessionStore::new())),
            signal_secret: Some(secret.clone()),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hmac_val = compute_hmac("peer-a", now, &secret);

        let body = AuthRequest {
            peer_id: "peer-a".into(),
            timestamp: now.to_string(),
            hmac: hmac_val,
        };

        let resp = auth_handler(State(app.clone()), Json(body)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(json["session_token"].as_str().unwrap().len() > 0);

        // Verify token was stored
        let sessions = app.sessions.read().await;
        assert_eq!(sessions.tokens.len(), 1);
    }

    #[tokio::test]
    async fn auth_invalid_hmac_returns_401() {
        let app = AppState {
            signaling: Arc::new(RwLock::new(SignalingState::new())),
            sessions: Arc::new(RwLock::new(SessionStore::new())),
            signal_secret: Some(b"real-secret".to_vec()),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let body = AuthRequest {
            peer_id: "peer-a".into(),
            timestamp: now.to_string(),
            hmac: "bogus-hmac-value".into(),
        };

        let resp = auth_handler(State(app), Json(body)).await.into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], "AUTH_FAILED");
    }

    #[tokio::test]
    async fn auth_no_secret_skips_hmac() {
        let app = AppState {
            signaling: Arc::new(RwLock::new(SignalingState::new())),
            sessions: Arc::new(RwLock::new(SessionStore::new())),
            signal_secret: None,
        };

        let body = AuthRequest {
            peer_id: "peer-a".into(),
            timestamp: "0".into(),
            hmac: "anything".into(),
        };

        let resp = auth_handler(State(app), Json(body)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn session_token_single_use() {
        let app = AppState {
            signaling: Arc::new(RwLock::new(SignalingState::new())),
            sessions: Arc::new(RwLock::new(SessionStore::new())),
            signal_secret: None,
        };

        // Get a session token
        let body = AuthRequest {
            peer_id: "peer-a".into(),
            timestamp: "0".into(),
            hmac: "".into(),
        };
        let resp = auth_handler(State(app.clone()), Json(body)).await.into_response();
        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token = json["session_token"].as_str().unwrap().to_string();

        // First consume succeeds
        {
            let mut sessions = app.sessions.write().await;
            let peer = sessions.consume(&token);
            assert_eq!(peer, Some("peer-a".into()));
        }

        // Second consume fails
        {
            let mut sessions = app.sessions.write().await;
            let peer = sessions.consume(&token);
            assert!(peer.is_none());
        }
    }

    #[tokio::test]
    async fn session_token_expired() {
        let app = AppState {
            signaling: Arc::new(RwLock::new(SignalingState::new())),
            sessions: Arc::new(RwLock::new(SessionStore::new())),
            signal_secret: None,
        };

        // Manually insert a backdated token
        {
            use crate::state::SessionEntry;
            let mut sessions = app.sessions.write().await;
            sessions.tokens.insert(
                "old-token".into(),
                SessionEntry {
                    peer_id: "peer-a".into(),
                    created_at: Instant::now()
                        - std::time::Duration::from_secs(SESSION_TTL_SECS + 1),
                    used: false,
                },
            );
        }

        // Consume should fail
        let mut sessions = app.sessions.write().await;
        assert!(sessions.consume("old-token").is_none());
    }

    #[tokio::test]
    async fn session_capacity_returns_503() {
        use crate::state::MAX_SESSIONS;

        let app = AppState {
            signaling: Arc::new(RwLock::new(SignalingState::new())),
            sessions: Arc::new(RwLock::new(SessionStore::new())),
            signal_secret: None,
        };

        // Fill up the session store
        {
            let mut sessions = app.sessions.write().await;
            for i in 0..MAX_SESSIONS {
                sessions.insert(format!("tok-{i}"), "peer".into());
            }
        }

        let body = AuthRequest {
            peer_id: "overflow-peer".into(),
            timestamp: "0".into(),
            hmac: "".into(),
        };
        let resp = auth_handler(State(app), Json(body)).await.into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], "CAPACITY");
    }

    // ─── Integration test: full happy-path scenario ───

    use ed25519_dalek::Signer;

    #[tokio::test]
    async fn happy_path_register_resolve_signal_leave() {
        let state = Arc::new(RwLock::new(SignalingState::new()));

        // Set up signing keys for two peers
        let key_a = SigningKey::generate(&mut rand::rngs::OsRng);
        let key_b = SigningKey::generate(&mut rand::rngs::OsRng);

        let _token_a = make_test_token_with_key("peer-a", &key_a, false);
        let _token_b = make_test_token_with_key("peer-b", &key_b, false);

        // Create channels for both peers
        let (tx_a, mut rx_a) = mpsc::unbounded_channel::<Message>();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel::<Message>();

        // ── Step 1: Peer A registers with hash X ──
        {
            let entry = PeerEntry {
                peer_id: "peer-a".to_string(),
                file_hashes: vec!["hashX".to_string()],
                multiaddr: Some("/ip4/192.168.1.1/tcp/9000/p2p/peer-a".to_string()),
                tx: tx_a.clone(),
                connected_at: Instant::now(),
            };
            let mut s = state.write().await;
            s.register(entry).unwrap();
        }

        // ── Step 2: Peer B resolves hash X → gets Peer A's info ──
        {
            let s = state.read().await;
            let entry = s.resolve("hashX").unwrap();
            assert_eq!(entry.peer_id, "peer-a");
            assert_eq!(
                entry.multiaddr.as_deref(),
                Some("/ip4/192.168.1.1/tcp/9000/p2p/peer-a")
            );
        }

        // ── Step 3: Peer B sends signal to Peer A → Peer A receives it ──
        {
            let s = state.read().await;
            let target = s.get_peer("peer-a").unwrap();
            let signal_msg = send_msg(&OutboundMessage::Signal {
                from_peer_id: "peer-b".to_string(),
                payload: serde_json::json!({"sdp": "offer..."}),
            });
            target.tx.send(signal_msg).unwrap();
        }

        // Peer A should have received the signal
        let received = rx_a.recv().await.unwrap();
        let text = match received {
            Message::Text(t) => t.to_string(),
            other => panic!("expected text, got {other:?}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "signal");
        assert_eq!(parsed["fromPeerId"], "peer-b");
        assert_eq!(parsed["payload"]["sdp"], "offer...");

        // ── Step 4: Peer A disconnects → Peer B receives PeerLeft ──
        // First register Peer B so they're in the peer list
        {
            let entry = PeerEntry {
                peer_id: "peer-b".to_string(),
                file_hashes: vec![],
                multiaddr: None,
                tx: tx_b.clone(),
                connected_at: Instant::now(),
            };
            let mut s = state.write().await;
            s.register(entry).unwrap();
        }

        // Simulate Peer A disconnect
        {
            let mut s = state.write().await;
            s.remove("peer-a");

            // Broadcast PeerLeft to remaining peers
            let peer_left = send_msg(&OutboundMessage::PeerLeft {
                peer_id: "peer-a".to_string(),
            });
            for entry in s.peers.values() {
                let _ = entry.tx.send(peer_left.clone());
            }
        }

        // Peer B should receive PeerLeft
        let received = rx_b.recv().await.unwrap();
        let text = match received {
            Message::Text(t) => t.to_string(),
            other => panic!("expected text, got {other:?}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "peer_left");
        assert_eq!(parsed["peerId"], "peer-a");

        // Verify hash X is no longer resolvable
        {
            let s = state.read().await;
            assert!(s.resolve("hashX").is_none());
            assert!(s.get_peer("peer-a").is_none());
        }
    }
}
