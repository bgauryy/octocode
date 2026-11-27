//! One HTTPS request of a URL capture, to a public address only. Each hop
//! resolves its host here, refuses any address that is not publicly
//! routable, and pins the connection to exactly the checked addresses, so a
//! second DNS answer cannot redirect it to a private network. A host that
//! does not resolve gets the same refusal as a private one, naming neither
//! an address nor which case it was. The request
//! carries no credential or cookie and follows no redirect itself; the body
//! is read within the request budget and a byte limit.
use super::Reply;
use crate::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use octocode_io::http::{ClientPolicy, Proxy};
use octocode_io::retry::{BodyError, read_bounded};
use octocode_runtime_base::policy::network::{is_public, not_public_refusal};
use octocode_runtime_base::providers::RuntimeClients;
use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use std::net::{IpAddr, SocketAddr};
use url::{Host, Url};

pub(super) const ACCEPTED: &str = "text/html, text/*;q=0.9, application/json;q=0.9, application/javascript;q=0.9, application/xml;q=0.9, */*;q=0.1";

/// One request, without following its redirect.
pub trait Fetch {
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
                .map_err(|_| not_public(&host))?
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
        let client = page_client(host, addresses)?;
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
                    BodyError::TooLarge => ClassificationError::new(
                        ErrorCode::ClassificationContextTooLarge,
                        format!(
                            "The page body exceeds the {} MiB capture limit; nothing was judged.",
                            self.budget.max_body_bytes / (1024 * 1024)
                        ),
                        "Select a smaller page or section, or supply the text as content.",
                    ),
                    BodyError::Transport => unreachable(host, "the body read failed"),
                    BodyError::Decode => unreachable(
                        host,
                        "its body does not decode as its Content-Encoding says",
                    ),
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

/// `addresses` when there is one and every one is publicly routable.
pub(super) fn public_addresses(
    host: &Host<String>,
    addresses: Vec<SocketAddr>,
) -> Result<Vec<SocketAddr>, ClassificationError> {
    if addresses.is_empty() || addresses.iter().any(|address| !is_public(address.ip())) {
        return Err(not_public(host));
    }
    Ok(addresses)
}

/// The one refusal for a host that does not resolve or is not public.
fn not_public(host: &Host<String>) -> ClassificationError {
    denied(&not_public_refusal(&host.to_string()))
}

pub(super) fn denied(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::SecurityValidationFailed,
        message,
        "Capture only public HTTPS pages; read internal pages with a local reader.",
    )
}

fn unreachable(host: &Host<String>, why: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextFailed,
        format!("The page at {host} could not be fetched: {why}."),
        "Check the URL, or supply the page text as content.",
    )
}

fn stopped(stop: BudgetStop) -> ClassificationError {
    match stop {
        BudgetStop::Deadline => ClassificationError::new(
            ErrorCode::Timeout,
            "The page was not fetched before the call's deadline.",
            super::PAGE_DEADLINE_HINT,
        ),
        BudgetStop::Cancelled => ClassificationError::new(
            ErrorCode::Cancelled,
            "The call stopped before its page was fetched.",
            octocode_runtime_base::response::rows::CANCELLED_HINT,
        ),
    }
}

/// Clients kept for reuse per runtime; the oldest is dropped past this count.
const MAX_PAGE_CLIENTS: usize = 16;

/// What a page client is pinned to: its proxy mode, host and checked addresses.
type PageClientKey = (Proxy, String, Vec<SocketAddr>);

/// The client for a page on `host`, pinned to exactly `addresses`, reused
/// across fetches on one runtime so a later page of the same host keeps its
/// pooled connection and TLS session. A different address set (a new DNS
/// answer) is a different key, so a reused client never connects anywhere
/// this call did not check.
fn page_client(
    host: &Host<String>,
    addresses: Vec<SocketAddr>,
) -> Result<reqwest::Client, ClassificationError> {
    static CLIENTS: RuntimeClients<PageClientKey> = RuntimeClients::new(MAX_PAGE_CLIENTS);
    cached_page_client(&CLIENTS, host, addresses, |policy| {
        octocode_io::http::client(&policy)
    })
}

/// [`page_client`] through `clients`, building a miss with `build`.
fn cached_page_client<E>(
    clients: &RuntimeClients<PageClientKey>,
    host: &Host<String>,
    addresses: Vec<SocketAddr>,
    build: impl FnOnce(ClientPolicy) -> Result<reqwest::Client, E>,
) -> Result<reqwest::Client, ClassificationError> {
    let mut key_addresses = addresses.clone();
    key_addresses.sort_unstable();
    key_addresses.dedup();
    let key = (
        Proxy::Direct,
        host.to_string().to_ascii_lowercase(),
        key_addresses,
    );
    clients
        .get(key, || build(page_policy(host, addresses)))
        .map_err(|_| {
            ClassificationError::new(
                ErrorCode::ClassificationContextFailed,
                "The page request could not be prepared.",
                "Retry; supply the page text as content if it keeps failing.",
            )
        })
}

/// The client policy of a page fetch: direct to `host`'s validated
/// `addresses`, and, as a request may have no deadline, ended only by a page
/// that stops sending ([`ClientPolicy::without_deadline`]).
fn page_policy(host: &Host<String>, addresses: Vec<SocketAddr>) -> ClientPolicy {
    ClientPolicy {
        proxy: Proxy::Direct,
        resolve: match host {
            Host::Domain(name) => Some((name.clone(), addresses)),
            _ => None,
        },
        user_agent: Some(concat!("octocode-native/", env!("CARGO_PKG_VERSION"))),
        ..ClientPolicy::without_deadline()
    }
}

#[cfg(test)]
mod tests;
