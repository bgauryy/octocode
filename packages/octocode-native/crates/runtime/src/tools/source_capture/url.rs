//! HTTPS page capture for clasify `source: {url}`.
//!
//! The page is fetched anonymously from a public address ([`fetch`]),
//! following at most [`MAX_REDIRECTS`] redirects, each re-checked as HTTPS
//! without credentials. Its text ([`extract`]: the whole page, or the
//! section its `#fragment` anchors) is redacted like every reader's output
//! and judged only when it fits the state limit whole. That exact text is
//! saved as an immutable, content-addressed file under the runtime's
//! capture storage, and the row's read lead is a `localFetch` of that file
//! at its snapshot, so a copied verification call reads the bytes that were
//! judged even after the live page changes.
//!
//! URL capture needs persistent storage, an available `localFetch`, and a
//! capture directory the local path policy lets `localFetch` read; without
//! them it fails before any network work.
mod decode;
mod extract;
mod fetch;
mod store;

use super::{Captured, failure};
use crate::execution::ExecutionContext;
use crate::security::ContentSecurity;
use crate::tools::judgment::ClassificationError;
use crate::tools::source_capture::SourceReader;
use decode::{decode, decode_fragment, extraction_failure};
use fetch::Fetch;
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::path::{Path, PathBuf};
pub(super) use store::Store;
use url::Url;

/// Redirects one capture follows.
const MAX_REDIRECTS: usize = 5;

/// One HTTP response as the capture needs it.
pub(super) struct Reply {
    pub status: u16,
    pub location: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Shared immutable capture storage: text and explicitly named images.
pub(crate) fn capture_root(home: &Path) -> PathBuf {
    home.join("tmp").join("capture").join("url")
}

pub(crate) fn capture(
    address: &str,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    max_chars: usize,
) -> Result<Captured, ClassificationError> {
    let target = parse(address)?;
    let store = Store::open(dispatcher)?;
    let network = fetch::Network {
        handle: dispatcher.handle(),
        accept: fetch::ACCEPTED,
        budget: octocode_io::budget::RequestBudget::new(
            context.deadline,
            context.cancellation.clone(),
            fetch::MAX_BODY_BYTES,
        ),
    };
    let redaction = Redaction {
        security: dispatcher.security(),
        emails: dispatcher.config().resolved.output.redact_emails,
    };
    let check = || context.check().map_err(stopped);
    let selection = select(&network, target, &redaction, &check, max_chars)?;
    store.save(selection)
}

/// Fetch one explicitly named image through the same anonymous, public-only
/// HTTPS acquisition as text. Embedded page resources are never followed.
pub(super) fn image(
    address: &str,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    max_bytes: usize,
) -> Result<(Vec<u8>, serde_json::Value), ClassificationError> {
    let target = parse(address)?;
    let network = fetch::Network {
        handle: dispatcher.handle(),
        budget: octocode_io::budget::RequestBudget::new(
            context.deadline,
            context.cancellation.clone(),
            max_bytes,
        ),
        accept: "image/png, image/jpeg",
    };
    select_image(&network, target)
}

fn select_image(
    fetcher: &impl Fetch,
    target: Url,
) -> Result<(Vec<u8>, serde_json::Value), ClassificationError> {
    let whole_image = |url: &Url| {
        if url.fragment().is_some() {
            Err(failure(
                ErrorCode::InvalidClassificationContext,
                "An image URL cannot select a fragment; the complete image is judged.",
                "Pass a direct image URL without a fragment.",
            ))
        } else {
            Ok(())
        }
    };
    whole_image(&target)?;
    let (url, reply) = follow(fetcher, target)?;
    whole_image(&url)?;
    let media = reply
        .content_type
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !matches!(media.as_str(), "image/png" | "image/jpeg") {
        return Err(failure(
            ErrorCode::ClassificationContextUnavailable,
            "The URL did not return PNG or JPEG image content; nothing was judged.",
            "Pass a direct PNG or JPEG URL, not a page embedding the image.",
        ));
    }
    Ok((reply.body, json!({"url":url.as_str()})))
}

/// An HTTPS URL without credentials.
fn parse(address: &str) -> Result<Url, ClassificationError> {
    let url = Url::parse(address).map_err(|_| {
        failure(
            ErrorCode::InvalidClassificationContext,
            "The source URL does not parse.",
            "Pass an absolute https:// URL.",
        )
    })?;
    check_target(&url)?;
    Ok(url)
}

fn check_target(url: &Url) -> Result<(), ClassificationError> {
    if url.scheme() != "https" {
        return Err(fetch::denied(&format!(
            "A URL source must use HTTPS, not {}.",
            url.scheme()
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(fetch::denied(
            "A URL source must not carry credentials; capture runs anonymously.",
        ));
    }
    Ok(())
}

struct Redaction<'a> {
    security: &'a ContentSecurity,
    emails: bool,
}

impl Redaction<'_> {
    fn apply(&self, text: &str) -> String {
        let text = self.security.sanitize_text(text, None).content;
        if self.emails {
            self.security.redact_emails(&text)
        } else {
            text
        }
    }
}

/// The captured selection: the final URL (with the fragment that selected
/// it), the section, and the exact newline-terminated text.
struct Selection {
    url: Url,
    section: Option<String>,
    text: String,
}

/// The call's deadline or cancellation as a capture failure.
fn stopped(stop: crate::execution::ExecutionError) -> ClassificationError {
    match stop {
        crate::execution::ExecutionError::Cancelled => failure(
            ErrorCode::Cancelled,
            "The call stopped before its page was captured.",
            crate::response::rows::CANCELLED_HINT,
        ),
        _ => failure(
            ErrorCode::Timeout,
            "The page was not captured before the call's deadline.",
            "Retry with more time, or supply the page text as state.",
        ),
    }
}

/// `check` is the call's deadline and cancellation, polled while parsing.
fn select(
    fetcher: &impl Fetch,
    target: Url,
    redaction: &Redaction<'_>,
    check: &dyn Fn() -> Result<(), ClassificationError>,
    max_chars: usize,
) -> Result<Selection, ClassificationError> {
    let (url, reply) = follow(fetcher, target)?;
    let (media, body) = decode(&reply)?;
    let fragment = url.fragment().map(decode_fragment);
    let (section, extracted) = match media.as_str() {
        "text/html" => {
            let stop = || check().is_err();
            (
                fragment.clone(),
                extract::html(&body, fragment.as_deref(), &stop),
            )
        }
        _ => (None, extract::plain(&body)),
    };
    let text = extracted.map_err(|why| extraction_failure(why, fragment.as_deref(), check))?;
    let text = redaction.apply(&text);
    if text.chars().count() > max_chars {
        return Err(super::too_large(
            max_chars,
            "The page text",
            "Add a #fragment that selects one section, or supply a smaller part as state.",
        ));
    }
    Ok(Selection { url, section, text })
}

/// The final URL and its successful reply, through at most
/// [`MAX_REDIRECTS`] redirects. A redirect without its own fragment keeps
/// the one it came from.
fn follow(fetcher: &impl Fetch, mut url: Url) -> Result<(Url, Reply), ClassificationError> {
    for hop in 0..=MAX_REDIRECTS {
        let reply = fetcher.get(&url)?;
        if (200..300).contains(&reply.status) {
            return Ok((url, reply));
        }
        if !(300..400).contains(&reply.status) {
            return Err(failure(
                ErrorCode::ClassificationContextFailed,
                format!(
                    "The page answered HTTP {}; nothing was judged.",
                    reply.status
                ),
                "Check the URL; a page behind sign-in cannot be captured anonymously.",
            ));
        }
        if hop == MAX_REDIRECTS {
            break;
        }
        let location = reply.location.as_deref().ok_or_else(|| {
            failure(
                ErrorCode::ClassificationContextFailed,
                format!(
                    "The page answered HTTP {} without a Location.",
                    reply.status
                ),
                "Check the URL.",
            )
        })?;
        let mut next = url.join(location).map_err(|_| {
            failure(
                ErrorCode::ClassificationContextFailed,
                "The page redirected to a URL that does not parse.",
                "Check the URL.",
            )
        })?;
        check_target(&next)?;
        if next.fragment().is_none() {
            next.set_fragment(url.fragment());
        }
        url = next;
    }
    Err(failure(
        ErrorCode::ClassificationContextFailed,
        format!("The page redirected more than {MAX_REDIRECTS} times; nothing was judged."),
        "Pass the final URL directly.",
    ))
}

#[cfg(test)]
mod tests;
