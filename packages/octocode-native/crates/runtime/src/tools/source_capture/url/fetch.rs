//! One HTTPS request of a URL capture, to a public address only. Each hop
//! resolves its host here, refuses any address that is not publicly
//! routable, and pins the connection to exactly the checked addresses, so a
//! second DNS answer cannot redirect it to a private network. The request
//! carries no credential or cookie and follows no redirect itself; the body
//! is read within the request budget and a byte limit.
use super::{Reply, failure};
use crate::policy::network::is_public;
use crate::tools::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use octocode_io::http::{ClientPolicy, Proxy};
use octocode_io::retry::{BodyError, read_bounded};
use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use std::net::{IpAddr, SocketAddr};
use url::{Host, Url};

/// The largest response body a capture reads.
pub(super) const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

pub(super) const ACCEPTED: &str = "text/html, text/plain;q=0.9, text/markdown;q=0.9";

/// One request, without following its redirect.
pub(super) trait Fetch {
    fn get(&self, url: &Url) -> Result<Reply, ClassificationError>;
}

/// The network fetcher: requests run on the runtime under `budget`.
pub(super) struct Network<'a> {
    pub handle: &'a tokio::runtime::Handle,
    pub budget: RequestBudget,
    pub accept: &'a str,
}

impl Fetch for Network<'_> {
    fn get(&self, url: &Url) -> Result<Reply, ClassificationError> {
        self.handle.block_on(self.request(url))
    }
}

impl Network<'_> {
    async fn request(&self, url: &Url) -> Result<Reply, ClassificationError> {
        let host = url
            .host()
            .ok_or_else(|| denied("The URL names no host."))?
            .to_owned();
        let port = url.port_or_known_default().unwrap_or(443);
        let addresses = match &host {
            Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(*ip), port)],
            Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(*ip), port)],
            Host::Domain(name) => self
                .budget
                .wait(tokio::net::lookup_host((name.as_str(), port)))
                .await
                .map_err(stopped)?
                .map_err(|_| unreachable(&host, "its name does not resolve"))?
                .collect(),
        };
        let addresses = public_addresses(&host, addresses)?;
        self.send(url, &host, addresses).await
    }

    /// The request itself, connected to exactly `addresses` (already checked).
    async fn send(
        &self,
        url: &Url,
        host: &Host<String>,
        addresses: Vec<SocketAddr>,
    ) -> Result<Reply, ClassificationError> {
        let policy = ClientPolicy {
            proxy: Proxy::Direct,
            resolve: match host {
                Host::Domain(name) => Some((name.clone(), addresses)),
                _ => None,
            },
            user_agent: Some(concat!("octocode-native/", env!("CARGO_PKG_VERSION"))),
            ..ClientPolicy::default()
        };
        let client = octocode_io::http::client(&policy).map_err(|_| {
            failure(
                ErrorCode::ClassificationContextFailed,
                "The page request could not be prepared.",
                "Retry; supply the page text as state if it keeps failing.",
            )
        })?;
        let mut target = url.clone();
        target.set_fragment(None);
        let response = self
            .budget
            .wait(client.get(target).header(ACCEPT, self.accept).send())
            .await
            .map_err(stopped)?
            .map_err(|_| unreachable(host, "the connection failed"))?;
        let status = response.status().as_u16();
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value: &reqwest::header::HeaderValue| value.to_str().ok())
                .map(str::to_owned)
        };
        let location = header(LOCATION);
        let content_type = header(CONTENT_TYPE);
        let body = if (200..300).contains(&status) {
            read_bounded(response, &self.budget, self.budget.max_body_bytes)
                .await
                .map_err(|error| match error {
                    BodyError::Stopped(stop) => stopped(stop),
                    BodyError::TooLarge => failure(
                        ErrorCode::ClassificationContextTooLarge,
                        format!(
                            "The page body exceeds the {} MiB capture limit; nothing was judged.",
                            self.budget.max_body_bytes / (1024 * 1024)
                        ),
                        "Select a smaller page or section, or supply the text as state.",
                    ),
                    BodyError::Transport => unreachable(host, "the body read failed"),
                })?
                .to_vec()
        } else {
            Vec::new()
        };
        Ok(Reply {
            status,
            location,
            content_type,
            body,
        })
    }
}

/// `addresses` when every one is publicly routable.
pub(super) fn public_addresses(
    host: &Host<String>,
    addresses: Vec<SocketAddr>,
) -> Result<Vec<SocketAddr>, ClassificationError> {
    if addresses.is_empty() {
        return Err(unreachable(host, "its name has no address"));
    }
    if let Some(blocked) = addresses.iter().find(|address| !is_public(address.ip())) {
        return Err(denied(&format!(
            "{host} resolves to {}, which is not a public address.",
            blocked.ip()
        )));
    }
    Ok(addresses)
}

pub(super) fn denied(message: &str) -> ClassificationError {
    failure(
        ErrorCode::SecurityValidationFailed,
        message,
        "Capture only public HTTPS pages; read internal pages with a local reader.",
    )
}

fn unreachable(host: &Host<String>, why: &str) -> ClassificationError {
    failure(
        ErrorCode::ClassificationContextFailed,
        format!("The page at {host} could not be fetched: {why}."),
        "Check the URL, or supply the page text as state.",
    )
}

fn stopped(stop: BudgetStop) -> ClassificationError {
    match stop {
        BudgetStop::Deadline => failure(
            ErrorCode::Timeout,
            "The page was not fetched before the call's deadline.",
            "Retry with more time, or supply the page text as state.",
        ),
        BudgetStop::Cancelled => failure(
            ErrorCode::Cancelled,
            "The call stopped before its page was fetched.",
            crate::response::rows::CANCELLED_HINT,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_internal_answer_denies_the_whole_host() {
        let host = Host::Domain("docs.example.org".to_owned());
        let mixed = vec![
            "93.184.216.34:443".parse().unwrap(),
            "10.0.0.5:443".parse().unwrap(),
        ];
        let error = public_addresses(&host, mixed).unwrap_err();
        assert_eq!(error.code, ErrorCode::SecurityValidationFailed);
        assert!(error.message.contains("10.0.0.5"), "{}", error.message);
        let error = public_addresses(&host, Vec::new()).unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
        let one = vec!["93.184.216.34:443".parse().unwrap()];
        assert_eq!(public_addresses(&host, one.clone()).unwrap(), one);
    }

    use std::net::TcpListener;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const NAME: &str = "docs.example.org";

    /// `route` on `server`, addressed by a public-looking name pinned to the
    /// loopback listener, as a real capture pins a name to its checked addresses.
    fn pinned(server: &MockServer, route: &str) -> (Url, Host<String>, Vec<SocketAddr>) {
        let address = *server.address();
        (
            Url::parse(&format!("http://{NAME}:{}{route}", address.port())).unwrap(),
            Host::Domain(NAME.to_owned()),
            vec![address],
        )
    }

    fn budgeted(handle: &tokio::runtime::Handle, budget: RequestBudget) -> Network<'_> {
        Network {
            handle,
            accept: ACCEPTED,
            budget,
        }
    }

    async fn fetch_error(
        server: &MockServer,
        route: &str,
        budget: RequestBudget,
    ) -> ClassificationError {
        let handle = tokio::runtime::Handle::current();
        let (url, host, addresses) = pinned(server, route);
        budgeted(&handle, budget)
            .send(&url, &host, addresses)
            .await
            .err()
            .expect("the request fails")
    }

    fn budget(limit: usize) -> RequestBudget {
        RequestBudget::with_timeout(Duration::from_secs(5), limit)
    }

    #[tokio::test]
    async fn a_reply_carries_status_headers_and_body_and_the_request_carries_no_identity() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("<p>hi</p>", "text/html"))
            .expect(1)
            .mount(&server)
            .await;
        let handle = tokio::runtime::Handle::current();
        let (url, host, addresses) = pinned(&server, "/page#section");
        let reply = budgeted(&handle, budget(MAX_BODY_BYTES))
            .send(&url, &host, addresses)
            .await
            .expect("a reply");
        assert_eq!(reply.status, 200);
        assert_eq!(reply.content_type.as_deref(), Some("text/html"));
        assert_eq!(reply.location, None);
        assert_eq!(reply.body, b"<p>hi</p>");
        let seen = server.received_requests().await.expect("recorded");
        let request = &seen[0];
        assert_eq!(
            request
                .headers
                .get("accept")
                .and_then(|value| value.to_str().ok()),
            Some(ACCEPTED)
        );
        let agent = request.headers.get("user-agent").expect("user agent");
        assert!(
            agent.to_str().unwrap().starts_with("octocode-native/"),
            "{agent:?}"
        );
        for name in ["cookie", "authorization"] {
            assert!(request.headers.get(name).is_none(), "{name} is never sent");
        }
        assert_eq!(
            request
                .headers
                .get("host")
                .and_then(|value| value.to_str().ok()),
            Some(format!("{NAME}:{}", server.address().port()).as_str()),
            "the name is kept for the Host header while the address is pinned"
        );
    }

    #[tokio::test]
    async fn a_redirect_is_reported_not_followed_and_an_error_status_has_no_body() {
        let server = MockServer::builder().start().await;
        Mock::given(path("/old"))
            .respond_with(ResponseTemplate::new(307).insert_header("location", "/elsewhere"))
            .mount(&server)
            .await;
        Mock::given(path("/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("no such page"))
            .mount(&server)
            .await;
        let handle = tokio::runtime::Handle::current();
        let network = budgeted(&handle, budget(MAX_BODY_BYTES));
        let (url, host, addresses) = pinned(&server, "/old");
        let moved = network.send(&url, &host, addresses).await.ok().unwrap();
        assert_eq!(moved.status, 307);
        assert_eq!(moved.location.as_deref(), Some("/elsewhere"));
        assert!(moved.body.is_empty());
        let (url, host, addresses) = pinned(&server, "/missing");
        let missing = network.send(&url, &host, addresses).await.ok().unwrap();
        assert_eq!(missing.status, 404);
        assert!(missing.body.is_empty(), "an error page is never read");
        let requested = server.received_requests().await.unwrap();
        assert_eq!(requested.len(), 2, "the Location was not requested");
    }

    #[tokio::test]
    async fn a_body_up_to_the_limit_is_read_and_one_byte_more_is_refused() {
        let server = MockServer::builder().start().await;
        Mock::given(path("/exact"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'a'; 1024]))
            .mount(&server)
            .await;
        Mock::given(path("/over"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'a'; MAX_BODY_BYTES + 1]))
            .mount(&server)
            .await;
        let handle = tokio::runtime::Handle::current();
        let (url, host, addresses) = pinned(&server, "/exact");
        let reply = budgeted(&handle, budget(1024))
            .send(&url, &host, addresses)
            .await
            .expect("a body at the limit fits");
        assert_eq!(reply.body.len(), 1024);
        let (url, host, addresses) = pinned(&server, "/exact");
        let error = budgeted(&handle, budget(1023))
            .send(&url, &host, addresses)
            .await
            .err()
            .expect("one byte over the limit");
        assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
        let error = fetch_error(&server, "/over", budget(MAX_BODY_BYTES)).await;
        assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
        assert!(
            error.message.contains("exceeds the 2 MiB capture limit"),
            "{}",
            error.message
        );
    }

    #[tokio::test]
    async fn a_refused_connection_names_the_host_and_asks_for_the_text_as_state() {
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        let handle = tokio::runtime::Handle::current();
        let url = Url::parse(&format!("http://{NAME}:{}/", closed.port())).unwrap();
        let error = budgeted(&handle, budget(MAX_BODY_BYTES))
            .send(&url, &Host::Domain(NAME.to_owned()), vec![closed])
            .await
            .err()
            .expect("nothing listens");
        assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
        assert!(
            error.message.contains(
                "The page at docs.example.org could not be fetched: the connection failed."
            ),
            "{}",
            error.message
        );
    }

    #[tokio::test]
    async fn a_slow_server_hits_the_deadline_and_a_cancelled_call_stops_first() {
        let server = MockServer::builder().start().await;
        Mock::given(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
        let short = RequestBudget::with_timeout(Duration::from_millis(200), MAX_BODY_BYTES);
        let error = fetch_error(&server, "/slow", short).await;
        assert_eq!(error.code, ErrorCode::Timeout);

        let cancelled = budget(MAX_BODY_BYTES);
        cancelled.cancellation.cancel();
        let error = fetch_error(&server, "/slow", cancelled).await;
        assert_eq!(error.code, ErrorCode::Cancelled);
    }

    #[tokio::test]
    async fn a_url_without_a_host_is_denied_before_any_lookup() {
        let handle = tokio::runtime::Handle::current();
        let error = budgeted(&handle, budget(MAX_BODY_BYTES))
            .request(&Url::parse("data:text/plain,hi").unwrap())
            .await
            .err()
            .expect("no host");
        assert_eq!(error.code, ErrorCode::SecurityValidationFailed);
        assert_eq!(error.message, "The URL names no host.");
    }

    fn network(runtime: &tokio::runtime::Runtime) -> Network<'_> {
        Network {
            handle: runtime.handle(),
            accept: ACCEPTED,
            budget: RequestBudget::with_timeout(std::time::Duration::from_secs(5), MAX_BODY_BYTES),
        }
    }

    #[test]
    fn a_loopback_or_private_host_is_refused_before_connecting() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        // Nothing listens on these ports: a connection attempt would fail
        // as classificationContextFailed, not as a denial.
        for address in [
            "https://127.0.0.1:9/",
            "https://[::1]:9/",
            "https://localhost:9/",
            "https://169.254.169.254/latest/meta-data/",
            "https://10.0.0.1:9/",
        ] {
            let error = network(&runtime)
                .get(&Url::parse(address).unwrap())
                .err()
                .unwrap();
            assert_eq!(
                error.code,
                ErrorCode::SecurityValidationFailed,
                "{address}: {error:?}"
            );
        }
    }
}
