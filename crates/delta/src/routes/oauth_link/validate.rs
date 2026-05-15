//! Pure-logic validators for the link-minecraft OAuth flow.
//!
//! The `begin` and `confirm` handlers BOTH run these — defense in depth, so
//! an attacker who somehow bypasses `begin` (e.g. by linking the user
//! straight to `/confirm`) still hits the same checks. Extracting the rules
//! here also makes them unit-testable without spinning up Rocket.
//!
//! What's intentionally fixed and not configurable:
//!   * Audience string — security-critical; making this changeable per-deploy
//!     would invite "use the audience from the other flow" mistakes.
//!   * TTL — 5 minutes is the longest a captured browser-redirect URL stays
//!     useful before the cosmetics backend rejects it as expired. Operators
//!     should not extend this.
//!   * `redirect_uri` regex — loopback-only by design (see comment below).

use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

/// Audience claim minted into the JWT and required by the cosmetics backend.
/// Distinct from `cosmetics-submission`, `decoration`, `earthsocial`, and
/// `company-internal` so a token from any other flow cannot be replayed
/// against the link endpoint (and vice versa).
pub const LINK_AUDIENCE: &str = "earthcosmetics-link";

/// JWT lifetime in seconds. **5 minutes.** Long enough to cover a slow
/// browser redirect + a sleepy laptop wake, short enough that a captured
/// token in a URL bar isn't useful for long. The cosmetics backend's
/// nonce dedup window is set to 10 minutes (2× this) for safety.
pub const LINK_JWT_TTL_SECS: u64 = 300;

/// Strict UUID v4-or-v3-or-anything-with-dashes parser. We don't enforce a
/// specific UUID version (Mojang historically issued v3 names then switched
/// to v4) — only the standard 8-4-4-4-12 hex-and-dash shape.
static MOJANG_UUID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .expect("valid regex")
});

/// State token: alphanumeric + `-` + `_` only. Restrictive so a poisoned
/// state can't smuggle quotes, control chars, or anything that might
/// mis-render in a log line or a URL fragment splice. The mod generates
/// state as `bytesToHex(...)` which is `[0-9a-f]+` — well within this set.
static STATE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Za-z0-9_-]+$").expect("valid regex"));

/// Bounds on the state nonce: must be present and a sane size.
const STATE_MIN_LEN: usize = 8;
const STATE_MAX_LEN: usize = 256;

/// Ephemeral port range we accept on the loopback redirect_uri. The mod
/// binds with `port=0` (kernel-assigned), which on Linux/Mac/Windows
/// resolves to an IANA-registered ephemeral port (>= 1024 in practice,
/// but we accept any valid port number 1..=65535 so we don't break on a
/// platform that picks something low). Port 0 itself is rejected — it
/// means "any port" in bind context but is never a connectable target.
const MIN_LOOPBACK_PORT: u16 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum LinkParamError {
    BadRedirectUri,
    BadState,
    BadMojangUuid,
}

impl LinkParamError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BadRedirectUri => "bad_redirect_uri",
            Self::BadState => "bad_state",
            Self::BadMojangUuid => "bad_mojang_uuid",
        }
    }
}

/// Validate the loopback redirect target. Numeric port range, exact host,
/// exact path. Rejects:
///   * any scheme other than `http`
///   * any host other than `127.0.0.1` literal (NOT `localhost` — DNS
///     rebinding could route to an external host)
///   * port 0 or anything outside `1..=65535` (the latter is enforced by
///     `url::Url` itself; `Url::port()` returns `Option<u16>`)
///   * any path other than exactly `/cb`
///   * any query string or fragment
///   * any userinfo component (`user@host`)
///
/// Parsing via `url::Url` rather than a fragile multi-thousand-char port
/// regex — `Url::port()` does the numeric range work for us.
pub fn validate_redirect_uri(uri: &str) -> Result<(), LinkParamError> {
    let parsed = Url::parse(uri).map_err(|_| LinkParamError::BadRedirectUri)?;
    if parsed.scheme() != "http" {
        return Err(LinkParamError::BadRedirectUri);
    }
    // host_str() returns the bare host without userinfo or port.
    if parsed.host_str() != Some("127.0.0.1") {
        return Err(LinkParamError::BadRedirectUri);
    }
    // Reject `user@127.0.0.1:port/cb` — non-empty userinfo would let an
    // attacker hide a different real host in the form `user@evil.com`
    // (defeated already by the host_str check above, but belt + braces).
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(LinkParamError::BadRedirectUri);
    }
    let Some(port) = parsed.port() else {
        // Without an explicit port we'd be redirecting to the http default
        // (80), which the mod never binds to.
        return Err(LinkParamError::BadRedirectUri);
    };
    if port < MIN_LOOPBACK_PORT {
        return Err(LinkParamError::BadRedirectUri);
    }
    if parsed.path() != "/cb" {
        return Err(LinkParamError::BadRedirectUri);
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(LinkParamError::BadRedirectUri);
    }
    Ok(())
}

pub fn validate_state(state: &str) -> Result<(), LinkParamError> {
    let len = state.len();
    if !(STATE_MIN_LEN..=STATE_MAX_LEN).contains(&len) {
        return Err(LinkParamError::BadState);
    }
    if !STATE_RE.is_match(state) {
        return Err(LinkParamError::BadState);
    }
    Ok(())
}

pub fn validate_mojang_uuid(uuid: &str) -> Result<(), LinkParamError> {
    if MOJANG_UUID_RE.is_match(uuid) {
        Ok(())
    } else {
        Err(LinkParamError::BadMojangUuid)
    }
}

/// Run all three validators in fixed order. Use in handlers as a single
/// short-circuiting check before doing any work.
pub fn validate_all(redirect_uri: &str, state: &str, mojang_uuid: &str) -> Result<(), LinkParamError> {
    validate_redirect_uri(redirect_uri)?;
    validate_state(state)?;
    validate_mojang_uuid(mojang_uuid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_with_arbitrary_port() {
        assert!(validate_redirect_uri("http://127.0.0.1:49999/cb").is_ok());
        assert!(validate_redirect_uri("http://127.0.0.1:65535/cb").is_ok());
        assert!(validate_redirect_uri("http://127.0.0.1:1/cb").is_ok());
    }

    #[test]
    fn rejects_localhost_hostname() {
        // localhost can be DNS-rebound to a non-loopback address; require
        // the literal IP so an attacker's external host can't intercept.
        assert!(validate_redirect_uri("http://localhost:49999/cb").is_err());
    }

    #[test]
    fn rejects_https_or_other_scheme() {
        assert!(validate_redirect_uri("https://127.0.0.1:49999/cb").is_err());
        assert!(validate_redirect_uri("ftp://127.0.0.1:49999/cb").is_err());
    }

    #[test]
    fn rejects_paths_other_than_cb() {
        assert!(validate_redirect_uri("http://127.0.0.1:49999/").is_err());
        assert!(validate_redirect_uri("http://127.0.0.1:49999/cb/").is_err());
        assert!(validate_redirect_uri("http://127.0.0.1:49999/cb?steal=1").is_err());
        assert!(validate_redirect_uri("http://127.0.0.1:49999/foo").is_err());
    }

    #[test]
    fn rejects_query_or_fragment_in_redirect() {
        assert!(validate_redirect_uri("http://127.0.0.1:49999/cb?x=1").is_err());
        assert!(validate_redirect_uri("http://127.0.0.1:49999/cb#x").is_err());
    }

    #[test]
    fn rejects_authority_with_userinfo() {
        // user:pass@127.0.0.1 would still match a naïve substring but the
        // anchored regex rejects it because the host segment isn't bare
        // 127.0.0.1 right after the scheme.
        assert!(validate_redirect_uri("http://user@127.0.0.1:49999/cb").is_err());
    }

    #[test]
    fn rejects_external_lookalike_host() {
        assert!(validate_redirect_uri("http://127.0.0.1.evil.com:80/cb").is_err());
        assert!(validate_redirect_uri("http://127x0x0x1:49999/cb").is_err());
    }

    #[test]
    fn rejects_missing_or_short_state() {
        assert!(validate_state("").is_err());
        assert!(validate_state("short").is_err());
        assert!(validate_state(&"a".repeat(STATE_MIN_LEN)).is_ok());
        assert!(validate_state(&"a".repeat(STATE_MAX_LEN)).is_ok());
        assert!(validate_state(&"a".repeat(STATE_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn rejects_state_with_disallowed_chars() {
        // Restrictive alphabet — no quotes, no control chars, no whitespace,
        // no slashes or dots that could confuse a downstream URL splice.
        assert!(validate_state("has space here ").is_err());
        assert!(validate_state("has\"quote_here").is_err());
        assert!(validate_state("has/slash_here").is_err());
        assert!(validate_state("has.dot_here_too").is_err());
        assert!(validate_state("has\nnewline_in").is_err());
        // Mod's actual format (32-char lowercase hex) round-trips.
        assert!(validate_state("0123456789abcdef0123456789abcdef").is_ok());
        assert!(validate_state("ABC_def-XYZ-1234567890").is_ok());
    }

    #[test]
    fn rejects_redirect_with_no_explicit_port() {
        // url::Url accepts http://127.0.0.1/cb (defaults port to 80) — we
        // require an explicit port so the mod's ephemeral binding is
        // unambiguous on the wire.
        assert!(validate_redirect_uri("http://127.0.0.1/cb").is_err());
    }

    #[test]
    fn rejects_port_zero() {
        // Port 0 is "any port" in bind context, never a real connectable
        // target. url::Url parses it but our explicit MIN_LOOPBACK_PORT
        // bound rejects it.
        assert!(validate_redirect_uri("http://127.0.0.1:0/cb").is_err());
    }

    #[test]
    fn rejects_port_above_u16_max() {
        // url::Url itself rejects ports > 65535 at parse time.
        assert!(validate_redirect_uri("http://127.0.0.1:99999/cb").is_err());
        assert!(validate_redirect_uri("http://127.0.0.1:65536/cb").is_err());
    }

    #[test]
    fn accepts_full_ephemeral_range() {
        assert!(validate_redirect_uri("http://127.0.0.1:1024/cb").is_ok());
        assert!(validate_redirect_uri("http://127.0.0.1:49152/cb").is_ok());
        assert!(validate_redirect_uri("http://127.0.0.1:65535/cb").is_ok());
    }

    #[test]
    fn accepts_well_formed_uuid_any_version() {
        assert!(validate_mojang_uuid("12345678-1234-1234-1234-123456789abc").is_ok());
        assert!(validate_mojang_uuid("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE").is_ok());
    }

    #[test]
    fn rejects_undashed_or_short_uuid() {
        assert!(validate_mojang_uuid("12345678123412341234123456789abc").is_err());
        assert!(validate_mojang_uuid("12345678-1234-1234-1234").is_err());
        assert!(validate_mojang_uuid("nope").is_err());
        assert!(validate_mojang_uuid("").is_err());
    }
}
