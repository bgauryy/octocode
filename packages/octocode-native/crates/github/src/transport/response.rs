//! Reading a response: bounded bodies, pagination links, the typed error
//! of a failed response, and the SAML SSO header.
use bytes::Bytes;
use reqwest::{StatusCode, header::HeaderMap};
use serde::Deserialize;
use url::Url;

use octocode_io::{budget::BudgetStop, retry::BodyError};

use super::RequestContext;
use crate::{GitHubEndpoint, ProviderError, ProviderErrorKind, ProviderErrorReason};

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
    octocode_io::retry::read_bounded(response, &context.budget, context.budget.max_body_bytes)
        .await
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
        })
}

pub(super) fn parse_next(
    headers: &HeaderMap,
    endpoint: &GitHubEndpoint,
) -> Result<Option<Url>, ProviderError> {
    let Some(value) = headers.get("link").and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let link = |relation: &str| {
        value
            .split(',')
            .find(|part| part.contains(relation))
            .map(|part| {
                part.trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
            })
    };
    let Some(raw) = link("rel=\"next\"") else {
        return Ok(None);
    };
    let next = Url::parse(raw).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub pagination link")
    })?;
    if !endpoint.permits(&next) {
        return Err(ProviderError::new(
            ProviderErrorKind::RedirectDenied,
            "GitHub pagination link changed origin",
        ));
    }
    let page = |url: &Url| {
        url.query_pairs()
            .find(|(key, _)| key == "page")
            .and_then(|(_, value)| value.parse::<u64>().ok())
    };
    // GitHub may advertise next=N+1 alongside last=N on a full final page.
    // Only comparable numbered links establish that next is past the end.
    if let Some(last) = link("rel=\"last\"").and_then(|raw| Url::parse(raw).ok())
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
    documentation_url: Option<String>,
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
        request_id: headers
            .get("x-github-request-id")
            .and_then(|v| v.to_str().ok())
            .map(Into::into),
        documentation_url: parsed.documentation_url.map(String::into_boxed_str),
        rate_limit: None,
        retryable: status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS,
        reason: missing_ref.then_some(ProviderErrorReason::RefNotFound),
        sso_url: sso_directive(headers, "required")
            .and_then(|parameters| sso_parameter(parameters, "url"))
            .filter(|url| Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")))
            .map(Into::into),
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
