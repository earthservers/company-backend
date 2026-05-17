use serde::{Serialize, Deserialize};

use super::client::Ping;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Authenticate { token: String },
    BeginTyping { channel: String },
    EndTyping { channel: String },
    Subscribe { server_id: String },
    Ping { data: Ping, responded: Option<()> },
    /// P2P voice signalling. Relayed by the server to other voice channel
    /// members as `VoiceSignalRelay`. The payload is opaque to the server
    /// (SDP offers/answers, ICE candidates, capability handshakes, etc.).
    /// If `target_user` is set, the relay is delivered only to that user;
    /// otherwise it is broadcast to every other voice channel member.
    VoiceSignal {
        channel_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_user: Option<String>,
        payload: serde_json::Value,
    },
}
