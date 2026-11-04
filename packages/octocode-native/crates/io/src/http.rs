//! HTTP client construction. Every client Octocode builds goes through
//! [`client`], so redirects are never followed and automatic retries are
//! never enabled by default; callers pick timeouts, proxy handling, and
//! DNS pinning through [`ClientPolicy`].

use std::net::SocketAddr;
use std::time::Duration;

mod trust;

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
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::time::{Duration, Instant};

    /// One request on a loopback listener that answers `reply`, then closes.
    fn serve_once(reply: &'static str) -> (SocketAddr, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(reply.as_bytes());
        });
        (addr, handle)
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    #[test]
    fn only_direct_or_pinned_policies_bypass_the_proxy() {
        let pinned = Some(("pinned.invalid".to_owned(), Vec::new()));
        let policy = |proxy, resolve| ClientPolicy {
            proxy,
            resolve,
            ..ClientPolicy::default()
        };
        assert!(!connects_directly(&policy(Proxy::System, None)));
        assert!(connects_directly(&policy(Proxy::Direct, None)));
        assert!(
            connects_directly(&policy(Proxy::System, pinned.clone())),
            "a pinned name must not be re-resolved by a proxy"
        );
        assert!(connects_directly(&policy(Proxy::Direct, pinned)));
    }

    #[test]
    fn a_redirect_is_returned_not_followed() {
        // A 307 would replay a POST body to its Location; the caller must see it.
        let (addr, server) = serve_once(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:1/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        let status = runtime().block_on(async {
            let client = client(&ClientPolicy::default()).expect("client");
            client
                .get(format!("http://{addr}/"))
                .send()
                .await
                .expect("send")
                .status()
        });
        server.join().expect("server");
        assert_eq!(status.as_u16(), 307);
    }

    #[test]
    fn a_stalled_peer_fails_within_the_read_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let _peer = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept");
            std::thread::sleep(Duration::from_secs(5));
        });
        let (result, elapsed) = runtime().block_on(async {
            let policy = ClientPolicy {
                read_timeout: Some(Duration::from_millis(200)),
                ..ClientPolicy::default()
            };
            let client = client(&policy).expect("client");
            // Platform client setup is separate from a request's read timeout.
            let started = Instant::now();
            let result = client.get(format!("http://{addr}/")).send().await;
            (result, started.elapsed())
        });
        let error = result.expect_err("a stalled peer must not hang the request");
        assert!(
            error.is_timeout(),
            "the stalled read must time out: {error}"
        );
        assert!(elapsed < Duration::from_secs(4), "request took {elapsed:?}");
    }

    #[test]
    fn a_dns_pin_connects_the_name_to_the_pinned_address() {
        let (addr, server) =
            serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        let body = runtime().block_on(async {
            let policy = ClientPolicy {
                proxy: Proxy::Direct,
                resolve: Some(("pinned.invalid".into(), vec![addr])),
                ..ClientPolicy::default()
            };
            let client = client(&policy).expect("client");
            let port = addr.port();
            client
                .get(format!("http://pinned.invalid:{port}/"))
                .send()
                .await
                .expect("send")
                .text()
                .await
                .expect("body")
        });
        server.join().expect("server");
        assert_eq!(body, "ok");
    }

    fn gzipped(plain: &[u8]) -> Vec<u8> {
        use flate2::{Compression, write::GzEncoder};
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(plain).expect("encode");
        encoder.finish().expect("finish")
    }

    async fn gzip_server(plain: &[u8]) -> (wiremock::MockServer, Vec<u8>) {
        let server = wiremock::MockServer::start().await;
        let wire = gzipped(plain);
        wiremock::Mock::given(wiremock::matchers::any())
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-encoding", "gzip")
                    .set_body_bytes(wire.clone()),
            )
            .mount(&server)
            .await;
        (server, wire)
    }

    #[tokio::test]
    async fn a_gzip_body_is_requested_and_decoded() {
        let plain = "{\"ok\":true}".repeat(200);
        let (server, wire) = gzip_server(plain.as_bytes()).await;
        let client = client(&ClientPolicy::default()).expect("client");
        let response = client.get(server.uri()).send().await.expect("send");
        let budget = crate::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20);
        let body = crate::retry::read_bounded(response, &budget, 1 << 20)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), plain.as_bytes());
        assert!(wire.len() < plain.len());
        let sent = server.received_requests().await.expect("requests");
        let accept = sent[0]
            .headers
            .get("accept-encoding")
            .expect("accept-encoding");
        assert!(accept.to_str().expect("text").contains("gzip"));
    }

    #[tokio::test]
    async fn a_gzip_body_over_the_limit_is_refused_by_its_decoded_size() {
        let plain = vec![0_u8; 4 << 20];
        let (server, wire) = gzip_server(&plain).await;
        assert!(wire.len() < 64 << 10, "the bomb is small on the wire");
        let client = client(&ClientPolicy::default()).expect("client");
        let response = client.get(server.uri()).send().await.expect("send");
        let budget = crate::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20);
        let read = crate::retry::read_bounded(response, &budget, 1 << 20).await;
        assert_eq!(read, Err(crate::retry::BodyError::TooLarge));
    }

    /// A body read under `gzip`, whose wire bytes `wire` are not what that
    /// header promises.
    async fn read_labelled_gzip(wire: Vec<u8>) -> Result<bytes::Bytes, crate::retry::BodyError> {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::any())
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-encoding", "gzip")
                    .set_body_bytes(wire),
            )
            .mount(&server)
            .await;
        let client = client(&ClientPolicy::default()).expect("client");
        let response = client.get(server.uri()).send().await.expect("send");
        let budget = crate::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20);
        crate::retry::read_bounded(response, &budget, 1 << 20).await
    }

    /// The server answered in full and the bytes do not decode: no network
    /// failure, and sending the request again gets the same bytes.
    #[tokio::test]
    async fn a_body_that_does_not_decode_as_labelled_is_a_decode_error() {
        let plain = b"{\"plain\":\"not gzip at all\"}".to_vec();
        let truncated = {
            let mut wire = gzipped(&b"{\"ok\":true}".repeat(200));
            wire.truncate(wire.len() / 2);
            wire
        };
        for wire in [plain, truncated] {
            assert_eq!(
                read_labelled_gzip(wire).await,
                Err(crate::retry::BodyError::Decode)
            );
        }
    }

    /// The connection dying inside a compressed body stays a network failure.
    #[test]
    fn a_connection_lost_inside_a_compressed_body_is_transport() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request).expect("request");
            let wire = gzipped(&b"{\"ok\":true}".repeat(200));
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-encoding: gzip\r\ncontent-length: {}\r\n\r\n",
                wire.len()
            );
            stream.write_all(head.as_bytes()).expect("head");
            stream.write_all(&wire[..wire.len() / 2]).expect("part");
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let read = runtime.block_on(async {
            let client = client(&ClientPolicy::default()).expect("client");
            let response = client
                .get(format!("http://{address}/"))
                .send()
                .await
                .expect("send");
            let budget =
                crate::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20);
            crate::retry::read_bounded(response, &budget, 1 << 20).await
        });
        server.join().expect("server");
        assert_eq!(read, Err(crate::retry::BodyError::Transport));
    }
}
