<div align="center">

<img src="https://company.earthservers.net/assets/company-text-logo.svg" alt="Company" height="80" />

<h1>Company Backend</h1>

**The privacy-first social platform with built-in monetization.**

[![License](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/earthservers/stoatchat-build?style=flat-square&logoColor=white)](https://github.com/earthservers/stoatchat-build/stargazers)
[![Forks](https://img.shields.io/github/forks/earthservers/stoatchat-build?style=flat-square&logoColor=white)](https://github.com/earthservers/stoatchat-build/network/members)
[![Issues](https://img.shields.io/github/issues/earthservers/stoatchat-build?style=flat-square&logoColor=white)](https://github.com/earthservers/stoatchat-build/issues)
[![Pull Requests](https://img.shields.io/github/issues-pr/earthservers/stoatchat-build?style=flat-square&logoColor=white)](https://github.com/earthservers/stoatchat-build/pulls)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)

</div>

> Company Backend is an AGPL-3.0 fork of [revoltchat/backend](https://github.com/revoltchat/backend), continued and extended for end-to-end encryption, peer-to-peer signaling, live voice and streaming, and a creator-economy decorations marketplace. Upstream copyright notices are preserved per AGPL §5.

## What Company Adds

On top of the Revolt foundation, this backend ships:

- **End-to-end encryption (E2EE)** — encrypted channel invitations, server-signed epoch bumps for forward secrecy, pairing attestation, and an isolated `signer` service for cryptographic operations. Encrypted file storage in `core/files`.
- **P2P signaling** — [`company-beacon-signal`](company-beacon-signal), a standalone Rust signaling service for peer-to-peer connections between clients.
- **Voice & live streaming** — LiveKit-backed voice channels via the [`voice-ingress`](crates/daemons/voice-ingress) daemon, live-stream routes, and viewer/ring management.
- **Decorations marketplace** — a full creator-economy system: submit, purchase, equip, moderate, and cash out cosmetic decorations. Includes studio submission flow and earnings tracking.
- **Stripe payments + coin economy** — Stripe integration backed by an internal coin currency for in-app purchases.
- **Cosmetic moderation** — dedicated moderation queue for user-generated cosmetics.
- **Onboarding flows** — guided onboarding for new accounts.

## Components

### Crates

| Crate                    | Path                                                              | Description                                                       |
| ------------------------ | ----------------------------------------------------------------- | ----------------------------------------------------------------- |
| `core/config`            | [crates/core/config](crates/core/config)                          | Configuration                                                     |
| `core/database`          | [crates/core/database](crates/core/database)                      | Database implementation (MongoDB)                                 |
| `core/files`             | [crates/core/files](crates/core/files)                            | S3 storage with file encryption                                   |
| `core/models`            | [crates/core/models](crates/core/models)                          | API models (incl. decorations, encrypted invites)                 |
| `core/permissions`       | [crates/core/permissions](crates/core/permissions)                | Permission logic                                                  |
| `core/presence`          | [crates/core/presence](crates/core/presence)                      | User presence                                                     |
| `core/result`            | [crates/core/result](crates/core/result)                          | Result and error types                                            |
| `core/coalesced`         | [crates/core/coalesced](crates/core/coalesced)                    | Coalescion service                                                |
| `delta`                  | [crates/delta](crates/delta)                                      | REST API (E2EE, voice, streams, decorations, payments routes)     |
| `bonfire`                | [crates/bonfire](crates/bonfire)                                  | WebSocket events server                                           |
| `services/january`       | [crates/services/january](crates/services/january)                | Link/embed proxy                                                  |
| `services/gifbox`        | [crates/services/gifbox](crates/services/gifbox)                  | Tenor GIF proxy                                                   |
| `services/autumn`        | [crates/services/autumn](crates/services/autumn)                  | File server (encrypted uploads)                                   |
| `daemons/crond`          | [crates/daemons/crond](crates/daemons/crond)                      | Scheduled data cleanup                                            |
| `daemons/pushd`          | [crates/daemons/pushd](crates/daemons/pushd)                      | Push notification daemon                                          |
| `daemons/voice-ingress`  | [crates/daemons/voice-ingress](crates/daemons/voice-ingress)      | LiveKit voice ingress daemon                                      |

### Sibling services (outside `crates/`)

| Service                  | Path                                              | Description                                                |
| ------------------------ | ------------------------------------------------- | ---------------------------------------------------------- |
| `company-beacon-signal`  | [company-beacon-signal](company-beacon-signal)    | P2P signaling service (Rust)                               |
| `signer`                 | [signer](signer)                                  | Isolated Python signing service for E2EE attestation       |

## Minimum Supported Rust Version

Rust 1.86.0 or higher.

## Development Guide

Before contributing, make yourself familiar with [our contribution guidelines](https://company.earthservers.net/developers/contrib) and the [technical documentation for this project](https://company.earthservers.net/developers/backend).

Before getting started, you'll want to install:

- mise
- Docker
- Git
- mold (optional, faster compilation)

> A **default.nix** is available for Nix users!
> Run `nix-shell` to activate mise.

As a heads-up, the development environment uses the following ports:

| Service                   |      Port      |
| ------------------------- | :------------: |
| MongoDB                   |     27017      |
| Redis                     |      6379      |
| MinIO                     |     14009      |
| Maildev                   | 14025<br>14080 |
| Company Web App           |     14701      |
| RabbitMQ                  | 5672<br>15672  |
| `crates/delta`            |     14702      |
| `crates/bonfire`          |     14703      |
| `crates/services/autumn`  |     14704      |
| `crates/services/january` |     14705      |
| `crates/services/gifbox`  |     14706      |

Now you can clone and build the project:

```bash
git clone https://github.com/earthservers/stoatchat-build company-backend
cd company-backend
mise build
```

A default configuration `Revolt.toml` is present in this project that is suited for development.

If you'd like to change anything, create a `Revolt.overrides.toml` file and specify relevant variables.

> [!TIP]
> Use Sentry to catch unexpected service errors:
>
> ```toml
> # Revolt.overrides.toml
> [sentry]
> api = "https://abc@your.sentry/1"
> events = "https://abc@your.sentry/1"
> files = "https://abc@your.sentry/1"
> proxy = "https://abc@your.sentry/1"
> ```

> [!TIP]
> If you have port conflicts on common services, you can try the following:
>
> ```yaml
> # compose.override.yml
> services:
>   redis:
>     ports: !override
>       - "14079:6379"
>
>   database:
>     ports: !override
>       - "14017:27017"
>
>   rabbit:
>     ports: !override
>       - "14072:5672"
>       - "14672:15672"
> ```
>
> And corresponding Revolt configuration:
>
> ```toml
> #     Revolt.overrides.toml
> # and Revolt.test-overrides.toml
> [database]
> mongodb = "mongodb://127.0.0.1:14017"
> redis = "redis://127.0.0.1:14079/"
>
> [rabbit]
> port = 14072
> ```

Then continue:

```bash
# start other necessary services
docker compose up -d

# run everything together
./scripts/start.sh
# .. or individually
# run the API server
cargo run --bin revolt-delta
# run the events server
cargo run --bin revolt-bonfire
# run the file server
cargo run --bin revolt-autumn
# run the proxy server
cargo run --bin revolt-january
# run the tenor proxy
cargo run --bin revolt-gifbox
# run the push daemon (not usually needed in regular development)
cargo run --bin revolt-pushd
# run the voice ingress daemon
cargo run --bin voice-ingress

# hint:
# mold -run <cargo build, cargo run, etc...>
# mold -run ./scripts/start.sh
```

You can start a web client by doing the following:

```bash
# if you do not have yarn yet and have a modern Node.js:
corepack enable

# clone the web client and run it:
git clone --recursive https://github.com/revoltchat/revite
cd revite
yarn
yarn build:deps
echo "VITE_API_URL=http://local.company.earthservers.net:14702" > .env.local
yarn dev --port 14701
```

Then go to http://local.company.earthservers.net:14701 to create an account/login.

When signing up, go to http://localhost:14080 to find confirmation/password reset emails.

## Deployment Guide

### Cutting new crate releases

Begin by bumping crate versions:

```bash
just patch # 0.0.X
just minor # 0.X.0
just major # X.0.0
```

Then commit the changes to package files.

Proceed to publish all the new crates:

```bash
just publish
```

### Cutting new binary releases

Tag and push a new release by running:

```bash
just release
```

If you have bumped the crate versions, proceed to [GitHub releases](https://github.com/earthservers/stoatchat-build/releases/new) to create a changelog.

## Testing

First, start the required services:

```sh
docker compose -f docker-compose.db.yml up -d
```

Now run tests for whichever database:

```sh
TEST_DB=REFERENCE cargo nextest run
TEST_DB=MONGODB cargo nextest run
```

## License

Company Backend is licensed under the [GNU Affero General Public License v3.0](LICENSE), the same license as the upstream [Revolt backend](https://github.com/revoltchat/backend) it is derived from.

**Individual crates may supply their own licenses!**

If you run a modified version of this code as a network service, AGPL §13 requires you to offer the corresponding source code to your users.

## Credits

This project is a continuation of the work done by the [Revolt](https://revolt.chat) team and contributors. We're grateful for the foundation they built. All Revolt copyright notices and attribution are preserved throughout the codebase.
