use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InboundMessage {
    Register {
        #[serde(rename = "peerId")]
        peer_id: String,
        #[serde(rename = "fileHashes")]
        file_hashes: Vec<String>,
        multiaddr: Option<String>,
        token: String,
    },
    Resolve {
        hash: String,
        token: String,
    },
    Signal {
        #[serde(rename = "toPeerId")]
        to_peer_id: String,
        payload: serde_json::Value,
        token: String,
    },
    Leave,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutboundMessage {
    Registered {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    Resolved {
        hash: String,
        #[serde(rename = "peerId")]
        peer_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        multiaddr: Option<String>,
    },
    NotFound {
        hash: String,
    },
    Signal {
        #[serde(rename = "fromPeerId")]
        from_peer_id: String,
        payload: serde_json::Value,
    },
    Error {
        code: String,
    },
    PeerLeft {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
}

impl OutboundMessage {
    pub fn error(code: &str) -> Self {
        Self::Error {
            code: code.to_string(),
        }
    }
}

// ─── Auth endpoint types ───

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]  
pub struct AuthRequest {
    pub peer_id: String,
    pub timestamp: String,
    pub hmac: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub session_token: String,
}

#[derive(Debug, Serialize)]
pub struct AuthError {
    pub code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_register() {
        let json = r#"{"type":"register","peerId":"abc","fileHashes":["h1","h2"],"token":"tok"}"#;
        let msg: InboundMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, InboundMessage::Register { .. }));
    }

    #[test]
    fn deserialize_leave() {
        let json = r#"{"type":"leave"}"#;
        let msg: InboundMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, InboundMessage::Leave));
    }

    #[test]
    fn serialize_registered() {
        let msg = OutboundMessage::Registered {
            peer_id: "abc".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"registered""#));
        assert!(json.contains(r#""peerId":"abc""#));
    }

    #[test]
    fn serialize_resolved_without_multiaddr() {
        let msg = OutboundMessage::Resolved {
            hash: "h1".into(),
            peer_id: "p1".into(),
            multiaddr: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("multiaddr"));
    }

    #[test]
    fn serialize_error() {
        let msg = OutboundMessage::error("AUTH_FAILED");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""code":"AUTH_FAILED""#));
    }

    #[test]
    fn serialize_auth_response() {
        let resp = AuthResponse {
            session_token: "test-uuid".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""session_token":"test-uuid""#));
    }

    #[test]
    fn serialize_auth_error() {
        let err = AuthError {
            code: "AUTH_FAILED".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains(r#""code":"AUTH_FAILED""#));
    }

    #[test]
    fn deserialize_auth_request() {
        let json = r#"{"peer_id":"p1","timestamp":"12345","hmac":"abc"}"#;
        let req: AuthRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.peer_id, "p1");
        assert_eq!(req.timestamp, "12345");
        assert_eq!(req.hmac, "abc");
    }
}
