mod handlers;
mod messages;
mod state;

use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    http::HeaderMap,
    response::IntoResponse,
    response::Json,
    routing::{get, post},
};
use http::StatusCode;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::info;

use handlers::verify_client_hmac;
use state::{SessionStore, SignalingState};

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub signaling: Arc<RwLock<SignalingState>>,
    pub sessions: Arc<RwLock<SessionStore>>,
    pub signal_secret: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Default)]
struct WsQueryParams {
    session: Option<String>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<WsQueryParams>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // Layer 1: HMAC client authentication (checked before upgrade)
    if app.signal_secret.is_some() {
        let secret = app.signal_secret.as_ref().unwrap();

        // Check if HMAC headers are present
        let hmac_header = headers
            .get("X-Company-Client-HMAC")
            .and_then(|v| v.to_str().ok());
        let timestamp = headers
            .get("X-Company-Timestamp")
            .and_then(|v| v.to_str().ok());
        let peer_id = headers
            .get("X-Company-Peer-Id")
            .and_then(|v| v.to_str().ok());

        match (hmac_header, timestamp, peer_id) {
            // All HMAC headers present — validate them
            (Some(hmac_val), Some(ts), Some(pid)) => {
                if !verify_client_hmac(pid, ts, hmac_val, secret) {
                    return Err((StatusCode::UNAUTHORIZED, "invalid hmac"));
                }
            }
            // HMAC headers missing — try session token from query param
            _ => {
                let session_token = params
                    .session
                    .as_deref()
                    .ok_or((StatusCode::UNAUTHORIZED, "missing auth"))?;
                let mut sessions = app.sessions.write().await;
                sessions
                    .consume(session_token)
                    .ok_or((StatusCode::UNAUTHORIZED, "invalid session"))?;
            }
        }
    }

    Ok(ws.on_upgrade(move |socket| handlers::handle_socket(socket, app.signaling)))
}

async fn config_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "stunServers": [
            "stun:stun.l.google.com:19302",
            "stun:stun1.l.google.com:19302"
        ],
        "signalingUrl": "wss://app.company.earthservers.net/p2p-signal",
        "protocolVersion": "1.0"
    }))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "company_beacon_signal=info".parse().unwrap()),
        )
        .json()
        .init();

    let port: u16 = std::env::var("SIGNAL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let signal_secret: Option<Vec<u8>> = std::env::var("SIGNAL_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.into_bytes());

    if signal_secret.is_some() {
        info!("HMAC auth enabled");
    } else {
        info!("HMAC auth disabled — set SIGNAL_SECRET for production");
    }

    let app_state = AppState {
        signaling: Arc::new(RwLock::new(SignalingState::new())),
        sessions: Arc::new(RwLock::new(SessionStore::new())),
        signal_secret,
    };

    let app = Router::new()
        .route("/signal", get(ws_handler))
        .route("/signal/auth", post(handlers::auth_handler))
        .route("/signal/config", get(config_handler))
        .with_state(app_state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    info!(%addr, "beacon signaling server starting");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    info!("beacon signaling server stopped");
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT");
    tokio::select! {
        _ = sigterm.recv() => info!("received SIGTERM"),
        _ = sigint.recv() => info!("received SIGINT"),
    }
}
