use company_config::Settings;
use revolt_rocket_okapi::{revolt_okapi::openapi3::OpenApi, settings::OpenApiSettings};
pub use rocket::http::Status;
pub use rocket::response::Redirect;
use rocket::{Build, Rocket};

mod admin;
mod assets;
mod auth_external;
mod bots;
mod channels;
mod cosmetics_moderation;
mod customisation;
mod decorations;
mod oauth_link;
mod devices;
mod interactions;
mod invites;
mod licenses;
mod onboard;
mod policy;
mod push;
mod root;
mod safety;
mod servers;
mod streams;
mod sync;
pub mod training;
mod users;
mod webhooks;

pub fn mount(config: Settings, mut rocket: Rocket<Build>) -> Rocket<Build> {
    let settings = OpenApiSettings::default();

    if config.features.webhooks_enabled {
        mount_endpoints_and_merged_docs! {
            rocket, "/".to_owned(), settings,
            "/" => (vec![], custom_openapi_spec()),
            "" => openapi_get_routes_spec![root::root],
            "/users" => users::routes(),
            "/bots" => bots::routes(),
            "/channels" => channels::routes(),
            "/servers" => servers::routes(),
            "/invites" => invites::routes(),
            "/custom" => customisation::routes(),
            "/safety" => safety::routes(),
            "/admin" => admin::routes(),
            "/auth/account" => rocket_authifier::routes::account::routes(),
            "/auth/session" => rocket_authifier::routes::session::routes(),
            "/auth/mfa" => rocket_authifier::routes::mfa::routes(),
            "/onboard" => onboard::routes(),
            "/policy" => policy::routes(),
            "/push" => push::routes(),
            "/sync" => sync::routes(),
            "/devices" => devices::routes(),
            "/webhooks" => webhooks::routes(),
            "/interactions" => interactions::routes(),

"/decorations" => decorations::routes(),
            "/cosmetics-moderation" => cosmetics_moderation::routes(),
            "/streams" => streams::routes(),
            "/assets" => assets::routes()
        };
    } else {
        mount_endpoints_and_merged_docs! {
            rocket, "/".to_owned(), settings,
            "/" => (vec![], custom_openapi_spec()),
            "" => openapi_get_routes_spec![root::root],
            "/users" => users::routes(),
            "/bots" => bots::routes(),
            "/channels" => channels::routes(),
            "/servers" => servers::routes(),
            "/invites" => invites::routes(),
            "/custom" => customisation::routes(),
            "/safety" => safety::routes(),
            "/admin" => admin::routes(),
            "/auth/account" => rocket_authifier::routes::account::routes(),
            "/auth/session" => rocket_authifier::routes::session::routes(),
            "/auth/mfa" => rocket_authifier::routes::mfa::routes(),
            "/onboard" => onboard::routes(),
            "/policy" => policy::routes(),
            "/push" => push::routes(),
            "/sync" => sync::routes(),
            "/devices" => devices::routes(),
            "/interactions" => interactions::routes(),

"/decorations" => decorations::routes(),
            "/cosmetics-moderation" => cosmetics_moderation::routes(),
            "/streams" => streams::routes(),
            "/assets" => assets::routes()
        };
    }

    if config.features.webhooks_enabled {
        mount_endpoints_and_merged_docs! {
            rocket, "/0.8".to_owned(), settings,
            "/" => (vec![], custom_openapi_spec()),
            "" => openapi_get_routes_spec![root::root],
            "/users" => users::routes(),
            "/bots" => bots::routes(),
            "/channels" => channels::routes(),
            "/servers" => servers::routes(),
            "/invites" => invites::routes(),
            "/custom" => customisation::routes(),
            "/safety" => safety::routes(),
            "/admin" => admin::routes(),
            "/auth/account" => rocket_authifier::routes::account::routes(),
            "/auth/session" => rocket_authifier::routes::session::routes(),
            "/auth/mfa" => rocket_authifier::routes::mfa::routes(),
            "/onboard" => onboard::routes(),
            "/push" => push::routes(),
            "/sync" => sync::routes(),
            "/devices" => devices::routes(),
            "/webhooks" => webhooks::routes(),
            "/interactions" => interactions::routes(),

"/decorations" => decorations::routes(),
            "/cosmetics-moderation" => cosmetics_moderation::routes(),
            "/streams" => streams::routes(),
            "/assets" => assets::routes()
        };
    } else {
        mount_endpoints_and_merged_docs! {
            rocket, "/0.8".to_owned(), settings,
            "/" => (vec![], custom_openapi_spec()),
            "" => openapi_get_routes_spec![root::root],
            "/users" => users::routes(),
            "/bots" => bots::routes(),
            "/channels" => channels::routes(),
            "/servers" => servers::routes(),
            "/invites" => invites::routes(),
            "/custom" => customisation::routes(),
            "/safety" => safety::routes(),
            "/admin" => admin::routes(),
            "/auth/account" => rocket_authifier::routes::account::routes(),
            "/auth/session" => rocket_authifier::routes::session::routes(),
            "/auth/mfa" => rocket_authifier::routes::mfa::routes(),
            "/onboard" => onboard::routes(),
            "/push" => push::routes(),
            "/sync" => sync::routes(),
            "/devices" => devices::routes(),
            "/interactions" => interactions::routes(),

"/decorations" => decorations::routes(),
            "/cosmetics-moderation" => cosmetics_moderation::routes(),
            "/streams" => streams::routes(),
            "/assets" => assets::routes()
        };
    }

    // Mount external-service auth routes (no OpenAPI — internal use only)
    rocket = rocket.mount("/auth", auth_external::routes());
    rocket = rocket.mount("/0.8/auth", auth_external::routes());

    // MC account-link OAuth-style flow (mounted at root: /oauth/link-minecraft
    // is the canonical URL the mod opens in the user's browser).
    rocket = rocket.mount("/", oauth_link::routes());
    rocket = rocket.mount("/0.8", oauth_link::routes());

// Mount training data pipeline routes (no OpenAPI, JWT auth)
    rocket = rocket.mount("/api/training", training::training_routes());

    // Mount license activation proxy (no OpenAPI, user auth)
    rocket = rocket.mount("/api/licenses", routes![licenses::activate_license]);

    rocket
}

fn custom_openapi_spec() -> OpenApi {
    use revolt_rocket_okapi::revolt_okapi::openapi3::*;

    let mut extensions = schemars::Map::new();
    extensions.insert(
        "x-logo".to_owned(),
        json!({
            "url": "https://company.earthservers.net/header.png",
            "altText": "Company Header"
        }),
    );

    extensions.insert(
        "x-tagGroups".to_owned(),
        json!([
          {
            "name": "Company",
            "tags": [
              "Core"
            ]
          },
          {
            "name": "Users",
            "tags": [
              "User Information",
              "Direct Messaging",
              "Relationships"
            ]
          },
          {
            "name": "Bots",
            "tags": [
              "Bots"
            ]
          },
          {
            "name": "Channels",
            "tags": [
              "Channel Information",
              "Channel Invites",
              "Channel Permissions",
              "Messaging",
              "Interactions",
              "Groups",
              "Voice",
              "Webhooks",
            ]
          },
          {
            "name": "Servers",
            "tags": [
              "Server Information",
              "Server Members",
              "Server Permissions"
            ]
          },
          {
            "name": "Invites",
            "tags": [
              "Invites"
            ]
          },
          {
            "name": "Customisation",
            "tags": [
              "Emojis"
            ]
          },
          {
            "name": "Platform Administration",
            "tags": [
              "Admin",
              "User Safety"
            ]
          },
          {
            "name": "Authentication",
            "tags": [
              "Account",
              "Session",
              "Onboarding",
              "MFA"
            ]
          },
          {
            "name": "Miscellaneous",
            "tags": [
              "Sync",
              "Web Push",
              "Devices"
            ]
          },
{
            "name": "Premium",
            "tags": [
              "Premium"
            ]
          },
          {
            "name": "Decorations",
            "tags": [
              "Decorations"
            ]
          },
          {
            "name": "Streams",
            "tags": [
              "Streams"
            ]
          },
          {
            "name": "Assets",
            "tags": [
              "Assets"
            ]
          }
        ]),
    );

    OpenApi {
        openapi: OpenApi::default_version(),
        info: Info {
            title: "Company API".to_owned(),
            description: Some("Open source user-first chat platform.".to_owned()),
            terms_of_service: Some("https://company.earthservers.net/terms".to_owned()),
            contact: Some(Contact {
                name: Some("Company Support".to_owned()),
                url: Some("https://company.earthservers.net".to_owned()),
                email: Some("admin@earthservers.net".to_owned()),
                ..Default::default()
            }),
            license: Some(License {
                name: "AGPLv3".to_owned(),
                url: Some("https://github.com/revoltchat/delta/blob/master/LICENSE".to_owned()),
                ..Default::default()
            }),
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        },
        servers: vec![
            Server {
                url: "https://api.company.earthservers.net".to_owned(),
                description: Some("Company Production".to_owned()),
                ..Default::default()
            },
            Server {
                url: "http://local.company.earthservers.net:14702".to_owned(),
                description: Some("Local Company Environment".to_owned()),
                ..Default::default()
            },
        ],
        external_docs: Some(ExternalDocs {
            url: "https://company.earthservers.net/developers".to_owned(),
            description: Some("Company Developer Documentation".to_owned()),
            ..Default::default()
        }),
        extensions,
        tags: vec![
            Tag {
                name: "Core".to_owned(),
                description: Some(
                    "Use in your applications to determine information about the Company node"
                        .to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "User Information".to_owned(),
                description: Some("Query and fetch users on Company".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Direct Messaging".to_owned(),
                description: Some("Direct message other users on Company".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Relationships".to_owned(),
                description: Some(
                    "Manage your friendships and block list on the platform".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Bots".to_owned(),
                description: Some("Create and edit bots".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Channel Information".to_owned(),
                description: Some("Query and fetch channels on Company".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Channel Invites".to_owned(),
                description: Some("Create and manage invites for channels".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Channel Permissions".to_owned(),
                description: Some("Manage permissions for channels".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Messaging".to_owned(),
                description: Some("Send and manipulate messages".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Groups".to_owned(),
                description: Some("Create, invite users and manipulate groups".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Voice".to_owned(),
                description: Some("Join and talk with other users".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Server Information".to_owned(),
                description: Some("Query and fetch servers on Company".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Server Members".to_owned(),
                description: Some("Find and edit server members".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Server Permissions".to_owned(),
                description: Some("Manage permissions for servers".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Invites".to_owned(),
                description: Some("View, join and delete invites".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Account".to_owned(),
                description: Some("Manage your account".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Session".to_owned(),
                description: Some("Create and manage sessions".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "MFA".to_owned(),
                description: Some("Multi-factor Authentication".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Onboarding".to_owned(),
                description: Some(
                    "After signing up to Company, users must pick a unique username".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Sync".to_owned(),
                description: Some("Upload and retrieve any JSON data between clients".to_owned()),
                ..Default::default()
            },
            Tag {
                name: "Web Push".to_owned(),
                description: Some(
                    "Subscribe to and receive Company push notifications while offline".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Devices".to_owned(),
                description: Some(
                    "List paired devices and sync E2E DM history between them".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Webhooks".to_owned(),
                description: Some("Send messages from 3rd party services".to_owned()),
                ..Default::default()
            },
Tag {
                name: "Premium".to_owned(),
                description: Some(
                    "Premium subscriptions and AI Companion license purchases".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Decorations".to_owned(),
                description: Some(
                    "Submit, browse, equip, and moderate profile decorations".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Streams".to_owned(),
                description: Some(
                    "Discover currently active public streams".to_owned(),
                ),
                ..Default::default()
            },
            Tag {
                name: "Assets".to_owned(),
                description: Some(
                    "Upload, serve, and manage binary assets (avatars, icons, emojis, banners)"
                        .to_owned(),
                ),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
