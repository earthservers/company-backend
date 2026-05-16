use company_config::config;
use company_result::Result;
use rocket::serde::json::Json;
use serde::Serialize;

use crate::util::epoch_bump::EPOCH_BUMP_AUDIENCE;

/// # hCaptcha Configuration
#[derive(Serialize, JsonSchema, Debug)]
pub struct CaptchaFeature {
    /// Whether captcha is enabled
    pub enabled: bool,
    /// Client key used for solving captcha
    pub key: String,
}

/// # Generic Service Configuration
#[derive(Serialize, JsonSchema, Debug)]
pub struct Feature {
    /// Whether the service is enabled
    pub enabled: bool,
    /// URL pointing to the service
    pub url: String,
}

/// # Information about a livekit node
#[derive(Serialize, JsonSchema, Debug)]
pub struct VoiceNode {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub public_url: String,
}

/// # Voice Server Configuration
#[derive(Serialize, JsonSchema, Debug)]
pub struct VoiceFeature {
    /// Whether voice is enabled
    pub enabled: bool,
    /// All livekit nodes
    pub nodes: Vec<VoiceNode>,
}

/// # Epoch Bump Signing Configuration
#[derive(Serialize, JsonSchema, Debug)]
pub struct EpochSigningFeature {
    /// Whether server-signed epoch bumps are enabled. When true,
    /// clients should reject any `_e2e_epoch_bump` that doesn't carry
    /// a valid token signed by the key below.
    pub enabled: bool,
    /// RS256 public key in PEM (SPKI) format. Empty when disabled.
    /// Clients import this once at startup and use it to verify the
    /// signature on every received epoch-bump token.
    pub public_key_pem: String,
    /// Expected `aud` claim on epoch bump tokens.
    pub audience: String,
}

/// # Feature Configuration
#[derive(Serialize, JsonSchema, Debug)]
pub struct RevoltFeatures {
    /// hCaptcha configuration
    pub captcha: CaptchaFeature,
    /// Whether email verification is enabled
    pub email: bool,
    /// Whether this server is invite only
    pub invite_only: bool,
    /// Asset serving configuration (replaces autumn)
    pub assets: Feature,
    /// File server service configuration (backward compat alias for assets)
    pub autumn: Feature,
    /// Proxy service configuration (deprecated, always disabled)
    pub january: Feature,
    /// Voice server configuration
    pub livekit: VoiceFeature,
    /// Server-signed epoch bump configuration. Clients use this to
    /// verify purge-triggered epoch bumps cryptographically.
    pub epoch_signing: EpochSigningFeature,
}

/// # Build Information
#[derive(Serialize, JsonSchema, Debug)]
pub struct BuildInformation {
    /// Commit Hash
    pub commit_sha: String,
    /// Commit Timestamp
    pub commit_timestamp: String,
    /// Git Semver
    pub semver: String,
    /// Git Origin URL
    pub origin_url: String,
    /// Build Timestamp
    pub timestamp: String,
}

/// # Server Configuration
#[derive(Serialize, JsonSchema, Debug)]
pub struct RevoltConfig {
    /// Company API Version
    pub revolt: String,
    /// Features enabled on this Company node
    pub features: RevoltFeatures,
    /// WebSocket URL
    pub ws: String,
    /// URL pointing to the client serving this node
    pub app: String,
    /// Web Push VAPID public key
    pub vapid: String,
    /// Build information
    pub build: BuildInformation,
}

/// # Query Node
///
/// Fetch the server configuration for this Company instance.
#[openapi(tag = "Core")]
#[get("/")]
pub async fn root() -> Result<Json<RevoltConfig>> {
    let config = config().await;

    Ok(Json(RevoltConfig {
        revolt: env!("CARGO_PKG_VERSION").to_string(),
        features: RevoltFeatures {
            captcha: CaptchaFeature {
                enabled: !config.api.security.captcha.hcaptcha_key.is_empty(),
                key: config.api.security.captcha.hcaptcha_sitekey.clone(),
            },
            email: !config.api.smtp.host.is_empty(),
            invite_only: config.api.registration.invite_only,
            assets: Feature {
                enabled: true,
                url: format!("{}/assets", config.hosts.api),
            },
            autumn: Feature {
                enabled: true,
                url: format!("{}/assets", config.hosts.api),
            },
            january: Feature {
                enabled: false,
                url: String::new(),
            },
            epoch_signing: {
                // Best-effort: read the public key file once at startup
                // and inline its contents. Empty string + enabled:false
                // if the file can't be read; clients then fall back to
                // refusing all epoch bumps (safe default).
                let path = &config.external_auth.public_key_path;
                match std::fs::read_to_string(path) {
                    Ok(pem) => EpochSigningFeature {
                        enabled: true,
                        public_key_pem: pem,
                        audience: EPOCH_BUMP_AUDIENCE.to_string(),
                    },
                    Err(err) => {
                        log::warn!(
                            "root: failed to read epoch signing public key at {path}: {err}"
                        );
                        EpochSigningFeature {
                            enabled: false,
                            public_key_pem: String::new(),
                            audience: EPOCH_BUMP_AUDIENCE.to_string(),
                        }
                    }
                }
            },
            livekit: VoiceFeature {
                enabled: !config.hosts.livekit.is_empty(),
                nodes: config
                    .api
                    .livekit
                    .nodes
                    .iter()
                    .filter(|(_, node)| !node.private)
                    .map(|(name, value)| VoiceNode {
                        name: name.clone(),
                        lat: value.lat,
                        lon: value.lon,
                        public_url: config
                            .hosts
                            .livekit
                            .get(name)
                            .expect("Missing corresponding host for voice node")
                            .clone(),
                    })
                    .collect(),
            },
        },
        ws: config.hosts.events,
        app: config.hosts.app,
        vapid: config.pushd.vapid.public_key,
        build: BuildInformation {
            commit_sha: option_env!("VERGEN_GIT_SHA")
                .unwrap_or_else(|| "<failed to generate>")
                .to_string(),
            commit_timestamp: option_env!("VERGEN_GIT_COMMIT_TIMESTAMP")
                .unwrap_or_else(|| "<failed to generate>")
                .to_string(),
            semver: option_env!("VERGEN_GIT_SEMVER")
                .unwrap_or_else(|| "<failed to generate>")
                .to_string(),
            origin_url: option_env!("GIT_ORIGIN_URL")
                .unwrap_or_else(|| "<failed to generate>")
                .to_string(),
            timestamp: option_env!("VERGEN_BUILD_TIMESTAMP")
                .unwrap_or_else(|| "<failed to generate>")
                .to_string(),
        },
    }))
}

#[cfg(test)]
mod test {
    use crate::rocket;
    use rocket::http::Status;

    #[rocket::async_test]
    async fn hello_world() {
        let harness = crate::util::test::TestHarness::new().await;
        let response = harness.client.get("/").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
    }

    #[rocket::async_test]
    async fn hello_world_concurrent() {
        let harness = crate::util::test::TestHarness::new().await;
        let response = harness.client.get("/").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
    }
}
