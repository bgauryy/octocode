//! Reading a response: bounded bodies, pagination links, the typed error
//! of a failed response, and the SAML SSO header.
use bytes::Bytes;
use reqwest::{StatusCode, header::HeaderMap};
use serde::Deserialize;
use url::Url;

use octocode_io::{
    budget::BudgetStop,
    retry::{BodyError, classify_status},
};

use super::RequestContext;
use crate::{ProviderError, ProviderErrorKind, ProviderErrorReason};

#[cfg(test)]
mod pagination_tests;

/// An error response's body, and why it is empty when it could not be read.
/// A cancel while reading it is a cancel, not an empty body classified by
/// status; an oversized body keeps the status classification.
pub(super) async fn read_error_body(
    response: reqwest::Response,
    context: &RequestContext,
) -> Result<(Bytes, Option<&'static str>), ProviderError> {
    match read_bounded(response, context).await {
        Ok(body) => Ok((body, None)),
        Err(error) if error.kind == ProviderErrorKind::Cancelled => Err(error),
        Err(error) if error.kind == ProviderErrorKind::ResponseTooLarge => {
            Ok((Bytes::new(), Some("error body exceeded limit")))
        }
        Err(_) => Ok((Bytes::new(), Some("error body could not be read"))),
    }
}

/// A response body within the request budget and its body limit.
pub(super) async fn read_bounded(
    response: reqwest::Response,
    context: &RequestContext,
) -> Result<Bytes, ProviderError> {
    // A retained answer shares this buffer, so it holds no growth headroom.
    octocode_io::retry::read_bounded(response, &context.budget, context.budget.max_body_bytes)
        .await
        .map(|body| Bytes::copy_from_slice(&body))
        .map_err(|error| match error {
            // A body that stalls after the headers is bounded by the
            // request deadline too, not only by the outer runtime timeout.
            BodyError::Stopped(BudgetStop::Deadline) => ProviderError::new(
                ProviderErrorKind::Timeout,
                "GitHub response body deadline exceeded",
            ),
            BodyError::Stopped(stop) => stop.into(),
            BodyError::TooLarge => ProviderError::new(
                ProviderErrorKind::ResponseTooLarge,
                "GitHub response exceeded configured byte limit",
            ),
            BodyError::Transport => ProviderError::new(
                ProviderErrorKind::Transport,
                "failed reading GitHub response",
            ),
            BodyError::Decode => ProviderError::new(
                ProviderErrorKind::Decode,
                "GitHub response body does not decode as its Content-Encoding says",
            ),
        })
}

/// The target of the `Link` header part carrying `relation`
/// (`rel="next"`), if any.
fn link_target<'a>(headers: &'a HeaderMap, relation: &str) -> Option<&'a str> {
    headers
        .get_all("link")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| super::headers::list(value, ','))
        .find_map(|part| {
            let (target, parameters) = part.strip_prefix('<')?.split_once('>')?;
            let matches = super::headers::list(parameters, ';').any(|parameter| {
                let Some((name, value)) = parameter.split_once('=') else {
                    return false;
                };
                name.trim().eq_ignore_ascii_case("rel")
                    && value
                        .trim()
                        .trim_matches('"')
                        .split_ascii_whitespace()
                        .any(|value| value.eq_ignore_ascii_case(relation))
            });
            matches.then_some(target)
        })
}

/// The `page` query value of a pagination link.
fn link_page(url: &Url) -> Option<u64> {
    url.query_pairs()
        .find(|(key, _)| key == "page")
        .and_then(|(_, value)| value.parse::<u64>().ok())
}

/// The last page number the `Link` header names (`rel="last"`): how many
/// pages the list holds at the request's `per_page`. `None` when the header
/// names none (a single page, or a provider that omits it).
pub(crate) fn last_page(headers: &HeaderMap) -> Option<usize> {
    let last = Url::parse(link_target(headers, "last")?).ok()?;
    link_page(&last).and_then(|page| usize::try_from(page).ok())
}

/// The next page the `Link` header names. A link to another origin (a proxy
/// forwarding GitHub's own) is still the fact that more rows exist, so the
/// page keeps it and states `hasMore`; it is never followed, because every
/// request is bound to the configured origin
/// ([`crate::GitHubEndpoint::permits`]), and callers page by their own URL.
pub(super) fn parse_next(headers: &HeaderMap) -> Result<Option<Url>, ProviderError> {
    let Some(raw) = link_target(headers, "next") else {
        return Ok(None);
    };
    let next = Url::parse(raw).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub pagination link")
    })?;
    let page = link_page;
    // GitHub may advertise next=N+1 alongside last=N on a full final page.
    // Only comparable numbered links establish that next is past the end.
    if let Some(last) = link_target(headers, "last").and_then(|raw| Url::parse(raw).ok())
        && last.origin() == next.origin()
        && last.path() == next.path()
        && last
            .query_pairs()
            .filter(|(key, _)| key != "page")
            .eq(next.query_pairs().filter(|(key, _)| key != "page"))
        && page(&next)
            .zip(page(&last))
            .is_some_and(|(next, last)| next > last)
    {
        return Ok(None);
    }
    Ok(Some(next))
}
#[derive(Deserialize, Default)]
struct ErrorBody {
    message: Option<String>,
    /// GitHub validation failures carry the specific cause here
    /// (`"abc" is not a numeric value`) under a generic `Validation Failed`.
    #[serde(default)]
    errors: Vec<ErrorDetail>,
}
#[derive(Deserialize, Default)]
struct ErrorDetail {
    message: Option<String>,
}
/// Attach the GitHub token as a bearer `Authorization` header. `bearer_auth`
/// marks the value sensitive, so `Debug` of the request redacts it.
pub(super) fn authorize(request: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    request.bearer_auth(token)
}

pub(super) fn response_error(
    status: StatusCode,
    headers: &HeaderMap,
    body: Bytes,
) -> ProviderError {
    let parsed: ErrorBody = serde_json::from_slice(&body).unwrap_or_default();
    // GitHub names a branch, tag, or SHA it cannot resolve "No commit found
    // for SHA: …" (422) or "No commit found for the ref …" (404): a missing
    // ref, typed once here so no tool matches the message text.
    let missing_ref = matches!(status.as_u16(), 404 | 422)
        && parsed
            .message
            .as_deref()
            .is_some_and(|message| message.starts_with("No commit found"));
    let kind = if missing_ref {
        ProviderErrorKind::NotFound
    } else {
        ProviderErrorKind::from_status(status.as_u16())
    };
    let code = status.as_u16();
    let validation_details: Vec<String> = parsed
        .errors
        .into_iter()
        .filter_map(|error| error.message)
        .map(|message| message.trim().to_owned())
        .filter(|message| !message.is_empty())
        .collect();
    let message = match (kind, parsed.message) {
        (ProviderErrorKind::Unavailable, Some(detail)) => {
            format!("GitHub resource blocked for legal reasons (HTTP 451): {detail}")
        }
        (ProviderErrorKind::Unavailable, None) => {
            "GitHub resource blocked for legal reasons (HTTP 451)".to_owned()
        }
        (ProviderErrorKind::HttpStatus, Some(detail)) => {
            format!("GitHub API returned HTTP {code}: {detail}")
        }
        (ProviderErrorKind::Validation, detail) if !validation_details.is_empty() => match detail {
            Some(detail) => format!("{detail}: {}", validation_details.join("; ")),
            None => validation_details.join("; "),
        },
        (_, Some(detail)) => detail,
        (_, None) => format!("GitHub API returned HTTP {code}"),
    };
    ProviderError {
        kind,
        message: message.into_boxed_str(),
        status: Some(status.as_u16()),
        rate_limit: None,
        retryable: classify_status(status.as_u16()).is_retryable(),
        reason: missing_ref.then_some(ProviderErrorReason::RefNotFound),
        sso_url: sso_directive(headers, "required")
            .and_then(|parameters| sso_parameter(parameters, "url"))
            .filter(|url| Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")))
            .map(Into::into),
        proxy: None,
    }
}

/// The parameters of an `x-github-sso: <directive>; k=v; …` header, when it
/// names `directive` (`required` or `partial-results`).
pub(super) fn sso_directive<'a>(headers: &'a HeaderMap, directive: &str) -> Option<&'a str> {
    let value = headers.get("x-github-sso")?.to_str().ok()?;
    let (head, parameters) = value.split_once(';').unwrap_or((value, ""));
    head.trim()
        .eq_ignore_ascii_case(directive)
        .then_some(parameters)
}

pub(super) fn sso_parameter<'a>(parameters: &'a str, name: &str) -> Option<&'a str> {
    parameters.split(';').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key.trim() == name).then(|| value.trim())
    })
}
