use std::{collections::HashMap, path::Path};

use cached::proc_macro::cached;
use config::{Config, File, FileFormat};
use futures_locks::RwLock;
use once_cell::sync::Lazy;
use serde::Deserialize;

#[cfg(feature = "sentry")]
pub use sentry::{capture_error, capture_message, Level};
#[cfg(feature = "anyhow")]
pub use sentry_anyhow::capture_anyhow;

#[cfg(all(feature = "report-macros", feature = "sentry"))]
#[macro_export]
macro_rules! report_error {
    ( $expr: expr, $error: ident $( $tt:tt )? ) => {
        $expr
            .inspect_err(|err| {
                $crate::capture_message(
                    &format!("{err:?} ({}:{}:{})", file!(), line!(), column!()),
                    $crate::Level::Error,
                );
            })
            .map_err(|_| ::company_result::create_error!($error))
    };
}

#[cfg(all(feature = "report-macros", feature = "sentry"))]
#[macro_export]
macro_rules! capture_internal_error {
    ( $expr: expr ) => {
        $crate::capture_message(
            &format!("{:?} ({}:{}:{})", $expr, file!(), line!(), column!()),
            $crate::Level::Error,
        );
    };
}

#[cfg(all(feature = "report-macros", feature = "sentry"))]
#[macro_export]
macro_rules! report_internal_error {
    ( $expr: expr ) => {
        $expr
            .inspect_err(|err| {
                $crate::capture_message(
                    &format!("{err:?} ({}:{}:{})", file!(), line!(), column!()),
                    $crate::Level::Error,
                );
            })
            .map_err(|_| ::company_result::create_error!(InternalError))
    };
}

/// Paths to search for configuration
static CONFIG_SEARCH_PATHS: [&str; 3] = [
    // current working directory
    "Revolt.toml",
    // current working directory - overrides file
    "Revolt.overrides.toml",
    // root directory, for Docker containers
    "/Revolt.toml",
];

/// Path to search for test overrides
static TEST_OVERRIDE_PATH: &str = "Revolt.test-overrides.toml";

/// Configuration builder
static CONFIG_BUILDER: Lazy<RwLock<Config>> = Lazy::new(|| {
    RwLock::new({
        let mut builder = Config::builder().add_source(File::from_str(
            include_str!("../Revolt.toml"),
            FileFormat::Toml,
        ));

        if std::env::var("TEST_DB").is_ok() {
            builder = builder.add_source(File::from_str(
                include_str!("../Revolt.test.toml"),
                FileFormat::Toml,
            ));

            // recursively search upwards for an overrides file (if there is one)
            if let Ok(cwd) = std::env::current_dir() {
                let mut path = Some(cwd.as_path());
                while let Some(current_path) = path {
                    let target_path = current_path.join(TEST_OVERRIDE_PATH);
                    if target_path.exists() {
                        builder = builder
                            .add_source(File::new(target_path.to_str().unwrap(), FileFormat::Toml));
                    }

                    path = current_path.parent();
                }
            }
        }

        let cwd = std::env::current_dir().unwrap();
        let mut cwd: Option<&Path> = Some(&cwd);

        while let Some(path) = cwd {
            for config_path in CONFIG_SEARCH_PATHS {
                let config_path = path.join(config_path);
                if config_path.exists() {
                    builder = builder
                        .add_source(File::new(config_path.to_str().unwrap(), FileFormat::Toml));
                }
            }

            cwd = path.parent();
        }

        builder.build().unwrap()
    })
});

#[derive(Deserialize, Debug, Clone)]
pub struct Database {
    pub mongodb: String,
    pub redis: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Rabbit {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Hosts {
    pub app: String,
    pub api: String,
    pub events: String,
    /// URL of the January embed-proxy service. Used by the message
    /// embed task to resolve external links (YouTube, Twitter, etc.)
    /// into structured Embed objects. Defaults to empty — when empty,
    /// embed processing silently no-ops, leaving messages without
    /// rich embeds.
    #[serde(default)]
    pub january: String,
    pub livekit: HashMap<String, String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiRegistration {
    pub invite_only: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiSmtp {
    pub host: String,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub reply_to: Option<String>,
    pub port: Option<i32>,
    pub use_tls: Option<bool>,
    pub use_starttls: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PushVapid {
    pub queue: String,
    pub private_key: String,
    pub public_key: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PushFcm {
    pub queue: String,
    pub key_type: String,
    pub project_id: String,
    pub private_key_id: String,
    pub private_key: String,
    pub client_email: String,
    pub client_id: String,
    pub auth_uri: String,
    pub token_uri: String,
    pub auth_provider_x509_cert_url: String,
    pub client_x509_cert_url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PushApn {
    pub queue: String,
    pub sandbox: bool,
    pub pkcs8: String,
    pub key_id: String,
    pub team_id: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiSecurityCaptcha {
    pub hcaptcha_key: String,
    pub hcaptcha_sitekey: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiSecurity {
    pub authifier_shield_key: String,
    pub voso_legacy_token: String,
    pub captcha: ApiSecurityCaptcha,
    pub trust_cloudflare: bool,
    pub easypwned: String,
    pub tenor_key: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiWorkers {
    pub max_concurrent_connections: usize,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiLiveKit {
    pub call_ring_duration: usize,
    /// LiveKit voice server configuration.
    /// Phase 3: Used for PUBLIC STREAMS ONLY. Private voice uses P2P WebRTC.
    pub nodes: HashMap<String, LiveKitNode>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LiveKitNode {
    pub url: String,
    pub lat: f64,
    pub lon: f64,
    pub key: String,
    pub secret: String,

    // whether to hide the node in the nodes list
    #[serde(default)]
    pub private: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApiUsers {
    pub early_adopter_cutoff: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Api {
    pub registration: ApiRegistration,
    pub smtp: ApiSmtp,
    pub security: ApiSecurity,
    pub workers: ApiWorkers,
    pub livekit: ApiLiveKit,
    pub users: ApiUsers,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Pushd {
    pub production: bool,
    pub exchange: String,
    pub mass_mention_chunk_size: usize,

    // Queues
    pub message_queue: String,
    pub mass_mention_queue: String,
    pub dm_call_queue: String,
    pub fr_accepted_queue: String,
    pub fr_received_queue: String,
    pub generic_queue: String,
    pub ack_queue: String,

    pub vapid: PushVapid,
    pub fcm: PushFcm,
    pub apn: PushApn,
}

impl Pushd {
    fn get_routing_key(&self, key: String) -> String {
        match self.production {
            true => key + "-prd",
            false => key + "-tst",
        }
    }

    pub fn get_ack_routing_key(&self) -> String {
        self.get_routing_key(self.ack_queue.clone())
    }

    pub fn get_message_routing_key(&self) -> String {
        self.get_routing_key(self.message_queue.clone())
    }

    pub fn get_mass_mention_routing_key(&self) -> String {
        self.get_routing_key(self.mass_mention_queue.clone())
    }

    pub fn get_dm_call_routing_key(&self) -> String {
        self.get_routing_key(self.dm_call_queue.clone())
    }

    pub fn get_fr_accepted_routing_key(&self) -> String {
        self.get_routing_key(self.fr_accepted_queue.clone())
    }

    pub fn get_fr_received_routing_key(&self) -> String {
        self.get_routing_key(self.fr_received_queue.clone())
    }

    pub fn get_generic_routing_key(&self) -> String {
        self.get_routing_key(self.generic_queue.clone())
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct FilesLimit {
    pub min_file_size: usize,
    pub min_resolution: [usize; 2],
    pub max_mega_pixels: usize,
    pub max_pixel_side: usize,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FilesS3 {
    pub endpoint: String,
    pub path_style_buckets: bool,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub default_bucket: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Files {
    pub encryption_key: String,
    pub webp_quality: f32,
    pub blocked_mime_types: Vec<String>,
    pub clamd_host: String,
    pub scan_mime_types: Vec<String>,

    pub limit: FilesLimit,
    pub preview: HashMap<String, [usize; 2]>,
    pub s3: FilesS3,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GlobalLimits {
    pub group_size: usize,
    pub message_embeds: usize,
    pub message_replies: usize,
    pub message_reactions: usize,
    pub server_emoji: usize,
    pub server_roles: usize,
    pub server_channels: usize,

    pub new_user_hours: usize,

    pub body_limit_size: usize,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FeaturesLimits {
    pub outgoing_friend_requests: usize,

    pub bots: usize,
    pub message_length: usize,
    pub message_attachments: usize,
    pub servers: usize,
    pub voice_quality: u32,
    /// Maximum bitrate for voice channels in bps (e.g. 128000 = 128 kbps)
    #[serde(default = "default_max_voice_bitrate")]
    pub max_voice_bitrate: u32,
    pub video: bool,
    pub video_resolution: [u32; 2],
    pub video_aspect_ratio: [f32; 2],

    pub file_upload_size_limit: HashMap<String, usize>,

    /// Maximum concurrent stream viewers (0 = unlimited)
    #[serde(default = "default_max_stream_viewers")]
    pub max_stream_viewers: usize,
    /// Maximum members per server owned by this user
    #[serde(default = "default_max_server_members")]
    pub max_server_members: usize,
}

fn default_max_voice_bitrate() -> u32 {
    128_000
}

fn default_max_stream_viewers() -> usize {
    10
}

fn default_max_server_members() -> usize {
    100
}

#[derive(Deserialize, Debug, Clone)]
pub struct FeaturesLimitsCollection {
    pub global: GlobalLimits,

    pub new_user: FeaturesLimits,
    pub default: FeaturesLimits,

    #[serde(flatten)]
    pub roles: HashMap<String, FeaturesLimits>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FeaturesAdvanced {
    #[serde(default)]
    pub process_message_delay_limit: u16,
}

impl Default for FeaturesAdvanced {
    fn default() -> Self {
        Self {
            process_message_delay_limit: 5,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Features {
    pub limits: FeaturesLimitsCollection,
    pub webhooks_enabled: bool,
    pub mass_mentions_send_notifications: bool,
    pub mass_mentions_enabled: bool,

    /// When true, DMs between peers with active DataChannels bypass the server.
    /// Stub: not yet implemented.
    #[serde(default)]
    pub p2p_direct_dm: bool,

    #[serde(default)]
    pub advanced: FeaturesAdvanced,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Sentry {
    pub api: String,
    pub events: String,
    pub voice_ingress: String,
    pub pushd: String,
    pub crond: String,
    pub gifbox: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Stripe {
    /// Stripe secret API key (sk_test_... or sk_live_...)
    #[serde(default)]
    pub secret_key: String,
    /// Stripe webhook signing secret (whsec_...)
    #[serde(default)]
    pub webhook_secret: String,
    /// Stripe publishable key (pk_test_... or pk_live_...)
    #[serde(default)]
    pub publishable_key: String,
    /// Stripe Price ID for Basic tier ($5/mo)
    #[serde(default)]
    pub price_basic_monthly: String,
    /// Stripe Price ID for Pro tier ($10/mo)
    #[serde(default)]
    pub price_pro_monthly: String,
    /// Stripe Price ID for Ultra tier ($20/mo)
    #[serde(default)]
    pub price_ultra_monthly: String,
    /// Stripe Price ID for AI Companion license (one-time)
    #[serde(default)]
    pub price_ai_companion: String,
    /// Stripe Price ID for decoration submission fee ($5 one-time)
    #[serde(default)]
    pub price_decoration_submission: String,
    /// URL to redirect after successful checkout
    #[serde(default = "default_stripe_success_url")]
    pub success_url: String,
    /// URL to redirect after cancelled checkout
    #[serde(default = "default_stripe_cancel_url")]
    pub cancel_url: String,
}

fn default_stripe_success_url() -> String {
    "http://local.company.earthservers.net/settings/subscribe?success=true".to_string()
}

fn default_stripe_cancel_url() -> String {
    "http://local.company.earthservers.net/settings/subscribe?cancelled=true".to_string()
}

impl Default for Stripe {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            webhook_secret: String::new(),
            publishable_key: String::new(),
            price_basic_monthly: String::new(),
            price_pro_monthly: String::new(),
            price_ultra_monthly: String::new(),
            price_ai_companion: String::new(),
            price_decoration_submission: String::new(),
            success_url: default_stripe_success_url(),
            cancel_url: default_stripe_cancel_url(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct LicenseServer {
    /// License server base URL
    #[serde(default = "default_license_server_url")]
    pub url: String,
    /// Admin API key for the license server
    #[serde(default)]
    pub admin_key: String,
}

fn default_license_server_url() -> String {
    "http://localhost:14800".to_string()
}

impl Default for LicenseServer {
    fn default() -> Self {
        Self {
            url: default_license_server_url(),
            admin_key: String::new(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Training {
    /// Path to RSA-4096 private key for decrypting client training data
    #[serde(default = "default_training_private_key_path")]
    pub private_key_path: String,
    /// Path to RSA-4096 public key distributed to clients
    #[serde(default = "default_training_public_key_path")]
    pub public_key_path: String,
    /// Hex-encoded AES-256 master key for database encryption (Layer 2)
    #[serde(default)]
    pub db_master_key: String,
    /// Secret for validating training consent JWTs
    #[serde(default)]
    pub jwt_secret: String,
}

fn default_training_private_key_path() -> String {
    "/opt/company/keys/training_private.pem".to_string()
}

fn default_training_public_key_path() -> String {
    "/opt/company/keys/training_public.pem".to_string()
}

impl Default for Training {
    fn default() -> Self {
        Self {
            private_key_path: default_training_private_key_path(),
            public_key_path: default_training_public_key_path(),
            db_master_key: String::new(),
            jwt_secret: String::new(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Cosmetics {
    /// HTTPS URL of the EarthCosmetics backend (without trailing slash), e.g.
    /// `https://cosmetics.earthservers.net`. Company API proxies all
    /// moderation + submission calls through this URL with a Company-minted
    /// RS256 JWT attached.
    #[serde(default)]
    pub backend_url: String,
    /// Path to RS256 private key used to sign cosmetics moderation/submission
    /// JWTs. The matching public key is deployed to the cosmetics backend as
    /// `COMPANY_JWT_PUBLIC_KEY_PEM`.
    #[serde(default = "default_cosmetics_private_key_path")]
    pub private_key_path: String,
    /// JWT lifetime in seconds. Short by design — minted on demand per request.
    #[serde(default = "default_cosmetics_jwt_ttl_secs")]
    pub jwt_ttl_secs: u64,
    /// Audience claim for moderator-action tokens. Must match the cosmetics
    /// backend's `MODERATOR_JWT_AUDIENCE` (defaults to "decoration").
    #[serde(default = "default_cosmetics_moderator_aud")]
    pub moderator_audience: String,
    /// Audience claim for creator-submission tokens. Must match the cosmetics
    /// backend's `SUBMISSION_JWT_AUDIENCE` (defaults to "cosmetics-submission").
    #[serde(default = "default_cosmetics_submitter_aud")]
    pub submitter_audience: String,
}

fn default_cosmetics_private_key_path() -> String {
    "/opt/company/keys/cosmetics_private.pem".to_string()
}
fn default_cosmetics_jwt_ttl_secs() -> u64 { 1800 } // 30 minutes
fn default_cosmetics_moderator_aud() -> String { "decoration".to_string() }
fn default_cosmetics_submitter_aud() -> String { "cosmetics-submission".to_string() }

impl Default for Cosmetics {
    fn default() -> Self {
        Self {
            backend_url: String::new(),
            private_key_path: default_cosmetics_private_key_path(),
            jwt_ttl_secs: default_cosmetics_jwt_ttl_secs(),
            moderator_audience: default_cosmetics_moderator_aud(),
            submitter_audience: default_cosmetics_submitter_aud(),
        }
    }
}

/// Auth for external Earth Servers services (EarthSocial, the cosmetics
/// backend's submission path, the mod, future Company-clients).
///
/// Reuses the cosmetics RS256 keypair — `aud` is the only scoping mechanism.
/// All routes that issue or verify these JWTs live under `/auth/external-*`
/// and the matching `ServiceIdentity` request guard in `util::external_auth`.
#[derive(Deserialize, Debug, Clone)]
pub struct ExternalAuth {
    /// RS256 private key path (shared with cosmetics). Used to sign all
    /// external-audience JWTs (user tokens and service tokens).
    #[serde(default = "default_cosmetics_private_key_path")]
    pub private_key_path: String,
    /// RS256 public key path. Used to verify the service tokens that Company
    /// issues to itself for backend-to-backend reads.
    #[serde(default = "default_external_public_key_path")]
    pub public_key_path: String,
    /// TTL for user-audience JWTs (aud = earthsocial, etc.). Short — the
    /// frontend silently refreshes via `/auth/external-token` using the
    /// shared session cookie before expiry.
    #[serde(default = "default_external_user_token_ttl_secs")]
    pub user_token_ttl_secs: u64,
    /// TTL for service tokens issued via `/auth/service-token`.
    #[serde(default = "default_external_service_token_ttl_secs")]
    pub service_token_ttl_secs: u64,
    /// Allow-list of audience strings for `/auth/external-token`. Reject
    /// requests for any other audience.
    #[serde(default = "default_external_allowed_audiences")]
    pub allowed_audiences: Vec<String>,
    /// Map of `client_id` -> `client_secret` for `/auth/service-token`. Each
    /// secret should be at least 16 chars and rotated manually (see deploy
    /// docs). Empty by default — no service clients provisioned.
    #[serde(default)]
    pub service_clients: HashMap<String, String>,
    /// Parent domain to set on the SSO session cookie, e.g.
    /// `.earthservers.net`. Empty disables the cookie write (header-only auth).
    #[serde(default = "default_external_cookie_domain")]
    pub session_cookie_domain: String,
    /// CORS origins allowed to call `/auth/external-*`. NOTE: this is purely
    /// advisory metadata — Rocket's CORS fairing must also be configured to
    /// match. Documented here so operators can audit the allow-list in one
    /// place.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

fn default_external_public_key_path() -> String {
    "/opt/company/keys/cosmetics_public.pem".to_string()
}
fn default_external_user_token_ttl_secs() -> u64 { 900 } // 15 minutes
fn default_external_service_token_ttl_secs() -> u64 { 3600 } // 1 hour
fn default_external_allowed_audiences() -> Vec<String> {
    vec!["earthsocial".to_string(), "earth-nexus".to_string()]
}
fn default_external_cookie_domain() -> String {
    ".earthservers.net".to_string()
}

impl Default for ExternalAuth {
    fn default() -> Self {
        Self {
            private_key_path: default_cosmetics_private_key_path(),
            public_key_path: default_external_public_key_path(),
            user_token_ttl_secs: default_external_user_token_ttl_secs(),
            service_token_ttl_secs: default_external_service_token_ttl_secs(),
            allowed_audiences: default_external_allowed_audiences(),
            service_clients: HashMap::new(),
            session_cookie_domain: default_external_cookie_domain(),
            allowed_origins: Vec::new(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    pub database: Database,
    pub rabbit: Rabbit,
    pub hosts: Hosts,
    pub api: Api,
    pub pushd: Pushd,
    pub files: Files,
    pub features: Features,
    pub sentry: Sentry,
    pub production: bool,
    #[serde(default)]
    pub stripe: Stripe,
    #[serde(default)]
    pub license_server: LicenseServer,
    #[serde(default)]
    pub training: Training,
    #[serde(default)]
    pub cosmetics: Cosmetics,
    #[serde(default)]
    pub external_auth: ExternalAuth,
    /// Earth Nexus base URL (without trailing slash). Company's
    /// `User::active_subscription_tier()` reads from a local mirror of
    /// Nexus's subscriptions table, refreshed via this endpoint on login,
    /// on a `subscription-refresh` route hit, or on a per-request staleness
    /// check. Leave empty in environments where Nexus isn't running to
    /// silently disable the refresh (everyone stays on whatever the local
    /// `user.subscription` field says, including `Free` for new users).
    #[serde(default)]
    pub nexus: Nexus,
    /// coturn / TURN credential minting. Used by the
    /// `POST /voice/turn-credentials` endpoint to issue
    /// time-limited HMAC-SHA1 credentials for clients that need
    /// non-LiveKit WebRTC relays (P2P voice, beacon files). Leave
    /// `static_auth_secret` empty to disable the endpoint (returns
    /// 503).
    #[serde(default)]
    pub turn: Turn,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Turn {
    /// Shared secret matching coturn's `static-auth-secret=` line.
    /// Used as the HMAC-SHA1 key when minting credentials.
    /// Empty disables the endpoint.
    #[serde(default)]
    pub static_auth_secret: String,
    /// TURN server URLs the client should use. Sent in every
    /// credential response so the client doesn't need a separate
    /// configuration source. Example: `["turns:turn.example:5349",
    /// "turn:turn.example:3478"]`.
    #[serde(default)]
    pub urls: Vec<String>,
    /// Optional coturn `realm=` value. Some clients send it
    /// alongside the username during the long-term-cred handshake;
    /// most ignore it when REST-style creds are used. Present here
    /// for compatibility / future use.
    #[serde(default)]
    pub realm: String,
    /// Lifetime (seconds) of issued credentials. Default 3600 (1h).
    /// The username is `<unix_timestamp_expiry>:<user_id>` so even
    /// if the response is intercepted, the creds expire after this
    /// window.
    #[serde(default = "default_turn_ttl_secs")]
    pub ttl_secs: u64,
}

fn default_turn_ttl_secs() -> u64 { 3600 }

impl Default for Turn {
    fn default() -> Self {
        Self {
            static_auth_secret: String::new(),
            urls: Vec::new(),
            realm: String::new(),
            ttl_secs: default_turn_ttl_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Nexus {
    /// HTTPS URL of the Earth Nexus billing service (without trailing slash),
    /// e.g. `https://api.nexus.earthservers.net`. When empty, subscription
    /// refresh from Nexus is disabled.
    #[serde(default)]
    pub base_url: String,
    /// How often (in seconds) Company auto-refreshes a user's subscription
    /// state from Nexus during request processing. Default 1800 (30min).
    /// The `POST /users/me/subscription-refresh` endpoint forces an
    /// immediate refresh regardless of this value.
    #[serde(default = "default_nexus_refresh_interval_secs")]
    pub refresh_interval_secs: u64,
}

fn default_nexus_refresh_interval_secs() -> u64 { 1800 }

impl Default for Nexus {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            refresh_interval_secs: default_nexus_refresh_interval_secs(),
        }
    }
}

impl Settings {
    pub fn preflight_checks(&self) {
        if self.api.smtp.host.is_empty() {
            log::warn!("No SMTP settings specified! Remember to configure email.");
        }

        if self.api.security.captcha.hcaptcha_key.is_empty() {
            log::warn!("No Captcha key specified! Remember to add hCaptcha key.");
        }
    }
}

pub async fn init() {
    println!(
        ":: Company Configuration ::\n\x1b[32m{:?}\x1b[0m",
        config().await
    );
}

pub async fn read() -> Config {
    CONFIG_BUILDER.read().await.clone()
}

#[cached(time = 30)]
pub async fn config() -> Settings {
    let mut config = read().await.try_deserialize::<Settings>().unwrap();

    // inject REDIS_URI for redis-kiss library
    if std::env::var("REDIS_URL").is_err() {
        std::env::set_var("REDIS_URI", config.database.redis.clone());
    }

    // auto-detect production nodes
    if config.hosts.api.contains("https") && config.hosts.api.contains("earthservers.net") {
        config.production = true;
    }

    config
}

/// Configure logging and common Rust variables
#[cfg(feature = "sentry")]
pub async fn setup_logging(release: &'static str, dsn: String) -> Option<sentry::ClientInitGuard> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    if std::env::var("ROCKET_ADDRESS").is_err() {
        std::env::set_var("ROCKET_ADDRESS", "0.0.0.0");
    }

    pretty_env_logger::init();
    log::info!("Starting {release}");

    if dsn.is_empty() {
        None
    } else {
        Some(sentry::init((
            dsn,
            sentry::ClientOptions {
                release: Some(release.into()),
                ..Default::default()
            },
        )))
    }
}

#[cfg(feature = "sentry")]
#[macro_export]
macro_rules! configure {
    ($application: ident) => {
        let config = $crate::config().await;
        let _sentry = $crate::setup_logging(
            concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION")),
            config.sentry.$application,
        )
        .await;
    };
}

#[cfg(feature = "test")]
#[cfg(test)]
mod tests {
    use crate::init;

    #[async_std::test]
    async fn it_works() {
        init().await;
    }
}
