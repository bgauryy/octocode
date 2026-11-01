//! One HTTPS request of a URL capture, to a public address only. Each hop
//! resolves its host here, refuses any address that is not publicly
//! routable, and pins the connection to exactly the checked addresses, so a
//! second DNS answer cannot redirect it to a private network. The request
//! carries no credential or cookie and follows no redirect itself; the body
//! is read within the request budget and a byte limit.
use super::Reply;
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
            ClassificationError::new(
                ErrorCode::ClassificationContextFailed,
                "The page request could not be prepared.",
                "Retry; supply the page text as content if it keeps failing.",
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
                    BodyError::TooLarge => ClassificationError::new(
                        ErrorCode::ClassificationContextTooLarge,
                        format!(
                            "The page body exceeds the {} MiB capture limit; nothing was judged.",
                            self.budget.max_body_bytes / (1024 * 1024)
                        ),
                        "Select a smaller page or section, or supply the text as content.",
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
            crate::response::rows::CANCELLED_HINT,
        ),
    }
}

#[cfg(test)]
mod tests;
