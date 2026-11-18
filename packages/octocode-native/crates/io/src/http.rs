//! HTTP client construction. Every client Octocode builds goes through
//! [`client`], so redirects are never followed and automatic retries are
//! never enabled by default; callers pick timeouts, proxy handling, and
//! DNS pinning through [`ClientPolicy`].

use std::net::SocketAddr;
use std::time::Duration;

mod date;
mod failure;
mod trust;
pub use date::parse_http_date;
pub use failure::SendFailure;

/// The first Age list member, in seconds. Overflow remains old rather than
/// becoming a cache miss that resets the age (RFC 9111 sections 1.2.2 and 5.1).
pub fn parse_age(value: &str) -> Option<u64> {
    let value = value.split(',').next()?.trim();
    parse_delta_seconds(value)
}

/// Parse HTTP delta-seconds without wrapping a syntactically valid overflow.
pub fn parse_delta_seconds(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(value.bytes().fold(0_u64, |seconds, byte| {
        seconds
            .saturating_mul(10)
            .saturating_add(u64::from(byte - b'0'))
    }))
}

/// Add the successful request's elapsed time to its upstream Age. HTTP Age
/// uses whole seconds; round up so a subsecond delay never extends freshness.
pub fn account_response_delay(headers: &mut reqwest::header::HeaderMap, delay: Duration) {
    use reqwest::header::{AGE, HeaderValue};
    let origin_age = headers
        .get(AGE)
        .and_then(|value| parse_age(value.to_str().ok()?))
        .unwrap_or(0);
    let seconds = delay
        .as_secs()
        .saturating_add(u64::from(delay.subsec_nanos() != 0));
    let age = origin_age.saturating_add(seconds);
    headers.insert(AGE, HeaderValue::from(age));
}

/// A configured trust store or HTTP client could not be initialized.
#[derive(Debug)]
pub enum ClientError {
    TrustStore(std::io::Error),
    Tls(reqwest::Error),
}
impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrustStore(error) => write!(formatter, "Configured TLS trust store: {error}"),
            Self::Tls(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TrustStore(error) => Some(error),
            Self::Tls(error) => Some(error),
        }
    }
}

/// Which proxy settings a client honors.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Proxy {
    /// The user's system or environment proxy.
    #[default]
    System,
    /// Connect directly: loopback hosts, or a name pinned to validated addresses.
    Direct,
}

/// The longest a request with no deadline of its own waits for its peer's next
/// bytes, the first answer byte included: a client's
/// [`ClientPolicy::read_timeout`], or the stall bound of a budget whose
/// requests are retried ([`crate::budget::RequestBudget::with_stall`]), which
/// ends the request rather than one attempt of it. It ends a dead or silent
/// socket, never flowing work: every read that arrives starts it again. It matches the request budget every surface had
/// before requests lost their fixed deadline, so no exchange that completed
/// then (a model thinking before its first byte) is cut by it now.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "Stall bound between reads, reset by every read: ends a silent socket, never flowing work or a request"
    )
)]
pub const READ_STALL: Duration = Duration::from_secs(300);

/// What differs between Octocode's clients. Redirects are never followed and
/// automatic retries are off in every client: callers that retry do it
/// explicitly, with their own budget.
#[derive(Clone, Debug, Default)]
pub struct ClientPolicy {
    /// Whole-request deadline, including the body.
    pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    /// Longest stall between reads.
    pub read_timeout: Option<Duration>,
    pub proxy: Proxy,
    /// Connect `host` to these addresses instead of resolving it.
    pub resolve: Option<(String, Vec<SocketAddr>)>,
    pub user_agent: Option<&'static str>,
}

impl ClientPolicy {
    /// The policy of a client whose requests have no deadline of their own
    /// (no caller budget): a peer silent for [`READ_STALL`] ends one, a peer
    /// still sending never does. Callers add what else differs.
    #[must_use]
    pub fn without_deadline() -> Self {
        Self {
            read_timeout: Some(READ_STALL),
            ..Self::default()
        }
    }
}

/// Process variables the HTTP client reads: proxies and the trust store.
/// A host that keeps one process for many calls (the CLI's warm server)
/// keys on them; a spawned network tool (git) is given them.
pub const NETWORK_ENV: &[&str] = &[
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// Whether `policy` bypasses every proxy. A pinned name always connects
/// directly: a proxy would resolve it again and silently bypass the pin.
fn connects_directly(policy: &ClientPolicy) -> bool {
    policy.proxy == Proxy::Direct || policy.resolve.is_some()
}

/// The proxy a client built for `policy` sends a request for `url` through,
/// as `scheme://host[:port]` with no userinfo (a proxy's password is never
/// stated); `None`: it connects directly. It reads the same system and
/// environment settings the client does, so a failure names the route it
/// took. A lookup of the operating system's settings, so call it only on a
/// failure.
#[must_use]
pub fn proxy_in_effect(policy: &ClientPolicy, url: &str) -> Option<String> {
    if connects_directly(policy) {
        return None;
    }
    named_proxy(
        &hyper_util::client::proxy::matcher::Matcher::from_system(),
        url,
    )
}

/// The proxy `matcher` routes `url` through, without its userinfo.
fn named_proxy(matcher: &hyper_util::client::proxy::matcher::Matcher, url: &str) -> Option<String> {
    let target = url.parse::<http::Uri>().ok()?;
    let proxy = matcher.intercept(&target)?;
    let proxy = proxy.uri();
    let host = proxy.host()?;
    let scheme = proxy.scheme_str().unwrap_or("http");
    Some(match proxy.port_u16() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

/// Build a client for `policy`, adding configured PEM roots to the platform
/// store. An unreadable or malformed explicit trust store fails construction.
pub fn client(policy: &ClientPolicy) -> Result<reqwest::Client, ClientError> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never());
    if let Some(timeout) = policy.timeout {
        builder = builder.timeout(timeout);
    }
    if let Some(timeout) = policy.connect_timeout {
        builder = builder.connect_timeout(timeout);
    }
    if let Some(timeout) = policy.read_timeout {
        builder = builder.read_timeout(timeout);
    }
    if let Some(agent) = policy.user_agent {
        builder = builder.user_agent(agent);
    }
    if connects_directly(policy) {
        builder = builder.no_proxy();
    }
    if let Some((host, addresses)) = &policy.resolve {
        builder = builder.resolve_to_addrs(host, addresses);
    }
    builder
        .tls_certs_merge(trust::configured_roots()?)
        .build()
        .map_err(ClientError::Tls)
}

#[cfg(test)]
mod tests;
