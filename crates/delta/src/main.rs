#[macro_use]
extern crate rocket;
#[macro_use]
extern crate revolt_rocket_okapi;
#[macro_use]
extern crate serde_json;

pub mod routes;
pub mod util;

use company_config::config;
use company_database::events::client::EventV1;
use company_database::AMQP;
use company_ratelimits::rocket as ratelimiter;
use rocket::{Build, Rocket};
use rocket_cors::{AllowedHeaders, AllowedOrigins, CorsOptions};
use rocket_prometheus::PrometheusMetrics;
use std::net::Ipv4Addr;
use std::str::FromStr;

use amqprs::{
    channel::ExchangeDeclareArguments,
    connection::{Connection, OpenConnectionArguments},
};
use async_std::channel::unbounded;
use authifier::AuthifierEvent;
use rocket::data::ToByteUnit;
use company_database::voice::VoiceClient;

use crate::routes::training::{DatabaseEncryption, TrainingConfig, TrainingKeyPair};
use crate::util::interaction_store::InteractionStore;
use crate::util::license_verification::LicenseVerifier;
use crate::util::user_events::UserEventBroadcaster;

pub async fn web() -> Rocket<Build> {
    // Get settings
    let config = config().await;

    // Ensure environment variables are present
    config.preflight_checks();

    // Setup database
    let db = company_database::DatabaseInfo::Auto.connect().await.unwrap();
    log::info!("database_here {db:?}");
    db.migrate_database().await.unwrap();

    // Spawn the Nexus subscription-state refresh task. No-ops when
    // config.nexus.base_url is empty (dev without Nexus running). This
    // is what catches renewals, payment failures, and Stripe-Portal
    // cancellations that the user-triggered /subscription-refresh flow
    // doesn't see.
    crate::util::nexus_refresh_task::spawn(
        db.clone(),
        config.nexus.base_url.clone(),
        config.nexus.refresh_interval_secs,
    );

    // Setup Authifier event channel
    let (_, receiver) = unbounded();

    // Setup Authifier
    let authifier = db.clone().to_authifier().await;

    // Launch a listener for Authifier events
    async_std::task::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            match &event {
                AuthifierEvent::CreateSession { .. } | AuthifierEvent::CreateAccount { .. } => {
                    EventV1::Auth(event).global().await
                }
                AuthifierEvent::DeleteSession { user_id, .. }
                | AuthifierEvent::DeleteAllSessions { user_id, .. } => {
                    let id = user_id.to_string();
                    EventV1::Auth(event).private(id).await
                }
            }
        }
    });

    // Configure CORS
    // When credentials are enabled, Access-Control-Allow-Origin cannot be "*".
    // We must list exact origins. The web app origin and Studio are allowed.
    // social.earthservers.net is the EarthSocial frontend — it calls
    // /auth/external-token and /auth/service-token with credentials.
    let exact_origins = AllowedOrigins::some_exact(&[
        "https://studio.earthservers.net",
        "https://app.company.earthservers.net",
        "https://company.earthservers.net",
        "https://social.earthservers.net",
        "https://earthservers.net",
    ]);

    let cors = CorsOptions {
        allowed_origins: exact_origins,
        allowed_methods: [
            "Get", "Put", "Post", "Delete", "Options", "Head", "Trace", "Connect", "Patch",
        ]
        .iter()
        .map(|s| FromStr::from_str(s).unwrap())
        .collect(),
        allowed_headers: AllowedHeaders::some(&[
            "Content-Type",
            "X-Session-Token",
            "X-Bot-Token",
            "Authorization",
        ]),
        allow_credentials: true,
        expose_headers: [
            "X-Ratelimit-Limit",
            "X-Ratelimit-Bucket",
            "X-Ratelimit-Remaining",
            "X-Ratelimit-Reset-After",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        ..Default::default()
    }
    .to_cors()
    .expect("Failed to create CORS.");

    // Configure Swagger
    let swagger = revolt_rocket_okapi::swagger_ui::make_swagger_ui(
        &revolt_rocket_okapi::swagger_ui::SwaggerUIConfig {
            url: "/openapi.json".to_owned(),
            ..Default::default()
        },
    )
    .into();

    let swagger_0_8 = revolt_rocket_okapi::swagger_ui::make_swagger_ui(
        &revolt_rocket_okapi::swagger_ui::SwaggerUIConfig {
            url: "/0.8/openapi.json".to_owned(),
            ..Default::default()
        },
    )
    .into();

    let swagger_0_8 = revolt_rocket_okapi::swagger_ui::make_swagger_ui(
        &revolt_rocket_okapi::swagger_ui::SwaggerUIConfig {
            url: "/0.8/openapi.json".to_owned(),
            ..Default::default()
        },
    )
    .into();

    // Voice handler
    let voice_client = VoiceClient::new(config.api.livekit.nodes.clone());
    // Configure Rabbit
    let connection = Connection::open(&OpenConnectionArguments::new(
        &config.rabbit.host,
        config.rabbit.port,
        &config.rabbit.username,
        &config.rabbit.password,
    ))
    .await
    .expect("Failed to connect to RabbitMQ");

    let channel = connection
        .open_channel(None)
        .await
        .expect("Failed to open RabbitMQ channel");

    channel
        .exchange_declare(
            ExchangeDeclareArguments::new(&config.pushd.exchange, "direct")
                .durable(true)
                .finish(),
        )
        .await
        .expect("Failed to declare exchange");

    let amqp = AMQP::new(connection, channel);

    // Launch background task workers
    company_database::tasks::start_workers(db.clone(), amqp.clone());

    // Interaction store for slash command state
    let interaction_store = InteractionStore::new();

    // In-process broadcaster for user-update SSE. Publishers (change_username,
    // edit_user, …) call `publish_user_update`; subscribers are SSE connections
    // from external services (EarthSocial) that hydrate their user mirrors.
    let user_events = UserEventBroadcaster::new();

    // Configure Rocket
    let rocket = rocket::build();
    let prometheus = PrometheusMetrics::new();

    // Ratelimits
    let ratelimits = ratelimiter::RatelimitStorage::new(util::ratelimits::DeltaRatelimits);

    // Stripe configuration
    let stripe_config = config.stripe.clone();

    // License verification for AI Companion bots
    let license_server_config = config.license_server.clone();
    let license_verifier = LicenseVerifier::new(config.license_server.url.clone());

    // Training data pipeline: load encryption keys if configured
    let training_config = TrainingConfig {
        private_key_path: config.training.private_key_path.clone(),
        public_key_path: config.training.public_key_path.clone(),
        db_master_key: config.training.db_master_key.clone(),
        jwt_secret: config.training.jwt_secret.clone(),
    };

    let mut rocket = routes::mount(config, rocket)
        .attach(prometheus.clone())
        .mount("/metrics", prometheus)
        .mount("/", rocket_cors::catch_all_options_routes())
        .mount("/", ratelimiter::routes())
        .mount("/swagger/", swagger)
        .mount("/0.8/swagger/", swagger_0_8)
        .manage(authifier)
        .manage(db)
        .manage(amqp)
        .manage(cors.clone())
        .manage(voice_client)
        .manage(interaction_store)
        .manage(user_events)
        .manage(ratelimits)
        .manage(stripe_config)
        .manage(license_verifier)
        .manage(license_server_config)
        .attach(ratelimiter::RatelimitFairing)
        .attach(cors)
        .configure(rocket::Config {
            limits: rocket::data::Limits::default()
                .limit("string", 5.megabytes())
                .limit("json", 25.megabytes())
                .limit("data-form", 25.megabytes()),
            address: Ipv4Addr::new(0, 0, 0, 0).into(),
            port: 14702,
            ..Default::default()
        });

    // Load training encryption keys (wrapped in Option so Rocket always has managed state)
    let training_keys: Option<TrainingKeyPair> = if !training_config.private_key_path.is_empty()
        && std::path::Path::new(&training_config.private_key_path).exists()
    {
        match TrainingKeyPair::load_from_files(
            &training_config.private_key_path,
            &training_config.public_key_path,
        ) {
            Ok(keys) => {
                log::info!("Training encryption keys loaded");
                Some(keys)
            }
            Err(e) => {
                log::warn!("Failed to load training keys: {e}");
                None
            }
        }
    } else {
        log::info!("Training pipeline: key files not found, endpoints will return 503 until configured");
        None
    };

    let db_enc: Option<DatabaseEncryption> = if !training_config.db_master_key.is_empty() {
        match DatabaseEncryption::from_hex(&training_config.db_master_key) {
            Ok(enc) => {
                log::info!("Training database encryption initialized");
                Some(enc)
            }
            Err(e) => {
                log::warn!("Failed to initialize training DB encryption: {e}");
                None
            }
        }
    } else {
        None
    };

    rocket = rocket
        .manage(training_keys)
        .manage(db_enc)
        .manage(training_config);
    rocket
}

#[launch]
async fn rocket() -> _ {
    // Configure logging and environment
    company_config::configure!(api);

    // Start web server
    web().await
}
