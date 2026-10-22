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
mod extract;
mod fetch;

use super::{Captured, failure};
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use crate::tools::judgment::ClassificationError;
use crate::tools::source_capture::SourceReader;
use fetch::Fetch;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

/// Redirects one capture follows.
const MAX_REDIRECTS: usize = 5;

/// The capture file inside its content-addressed directory.
const CAPTURE_FILE: &str = "source.txt";

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
                "invalidClassificationContext",
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
            "classificationContextUnavailable",
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
            "invalidClassificationContext",
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
            "cancelled",
            "The call stopped before its page was captured.",
            crate::response::rows::CANCELLED_HINT,
        ),
        _ => failure(
            "timeout",
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
        return Err(failure(
            "classificationContextTooLarge",
            format!(
                "The page text exceeds the {max_chars}-character state limit; nothing was judged."
            ),
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
                "classificationContextFailed",
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
                "classificationContextFailed",
                format!(
                    "The page answered HTTP {} without a Location.",
                    reply.status
                ),
                "Check the URL.",
            )
        })?;
        let mut next = url.join(location).map_err(|_| {
            failure(
                "classificationContextFailed",
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
        "classificationContextFailed",
        format!("The page redirected more than {MAX_REDIRECTS} times; nothing was judged."),
        "Pass the final URL directly.",
    ))
}

/// The reply's media type and UTF-8 body. Only HTML, plain text and
/// Markdown are captured; anything else (binary documents, other charsets)
/// is unsupported, never judged from a lossy decoding.
fn decode(reply: &Reply) -> Result<(String, String), ClassificationError> {
    let content_type = reply.content_type.as_deref().unwrap_or_default();
    let mut parameters = content_type.split(';');
    let media = parameters
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !matches!(media.as_str(), "text/html" | "text/plain" | "text/markdown") {
        return Err(unsupported(&format!(
            "The page is {}, not HTML or text.",
            if media.is_empty() {
                "untyped"
            } else {
                media.as_str()
            }
        )));
    }
    let charset = parameters.find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"').to_ascii_lowercase())
    });
    if let Some(charset) = charset
        && !matches!(charset.as_str(), "utf-8" | "utf8" | "us-ascii")
    {
        return Err(unsupported(&format!(
            "The page is encoded as {charset}; only UTF-8 pages are captured."
        )));
    }
    if octocode_engine::text::sniff(&reply.body) == octocode_engine::text::BinaryVerdict::Binary {
        return Err(unsupported("The page body is binary."));
    }
    // The engine's one leading-BOM rule: a mark starting the body is not text.
    let body = &reply.body[octocode_engine::text::bom_len(&reply.body)..];
    let body =
        std::str::from_utf8(body).map_err(|_| unsupported("The page body is not valid UTF-8."))?;
    Ok((media, body.to_owned()))
}

fn unsupported(message: &str) -> ClassificationError {
    failure(
        "classificationContextUnavailable",
        message,
        "Capture an HTML or text page, or supply the content as state.",
    )
}

fn extraction_failure(
    why: extract::Failure,
    fragment: Option<&str>,
    check: &dyn Fn() -> Result<(), ClassificationError>,
) -> ClassificationError {
    let fragment = fragment.unwrap_or_default();
    match why {
        // The check that stopped the parse says why.
        extract::Failure::Stopped => check()
            .err()
            .unwrap_or_else(|| stopped(crate::execution::ExecutionError::Timeout)),
        extract::Failure::Empty { scripted: true } => failure(
            "classificationContextUnavailable",
            "The page has no static text: it builds its content with scripts.",
            "Read the rendered page in a browser and supply its text as state.",
        ),
        extract::Failure::Empty { scripted: false } => failure(
            "classificationContextEmpty",
            "The page has no text; nothing was judged.",
            "Check the URL.",
        ),
        extract::Failure::MissingAnchor => failure(
            "classificationContextEmpty",
            format!("No element on the page is anchored #{fragment}; nothing was judged."),
            "Use an anchor the page defines, or drop the fragment to capture the whole page.",
        ),
        extract::Failure::AmbiguousAnchor(count) => failure(
            "classificationContextEmpty",
            format!("{count} elements on the page are anchored #{fragment}; nothing was judged."),
            "Use an anchor the page defines once, or drop the fragment.",
        ),
        extract::Failure::EmptySection => failure(
            "classificationContextEmpty",
            format!("The section anchored #{fragment} has no text; nothing was judged."),
            "Use another anchor, or drop the fragment to capture the whole page.",
        ),
    }
}

/// A percent-decoded fragment; the raw fragment when it does not decode.
fn decode_fragment(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                index += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| raw.to_owned())
}

/// The runtime's URL capture storage.
pub(super) struct Store<'a> {
    root: PathBuf,
    ttl: Duration,
    paths: &'a PathPolicy,
}

impl<'a> Store<'a> {
    /// The storage, when a capture can be saved and read back here.
    pub(super) fn open(dispatcher: &'a dyn SourceReader) -> Result<Self, ClassificationError> {
        if !crate::config::is_persistent_storage_enabled(&dispatcher.config().resolved) {
            return Err(failure(
                "classificationContextUnavailable",
                "Source captures need persistent storage to save the exact evidence; storage.mode is memory.",
                "Use persistent storage for unread source captures.",
            ));
        }
        let reader = ToolId::LocalFetch.as_str();
        if !dispatcher.availability().is_available(reader) {
            return Err(failure(
                "classificationContextUnavailable",
                format!(
                    "Source capture storage requires {reader}, which is not available in this runtime."
                ),
                "Enable localFetch for source capture storage.",
            ));
        }
        Self::at(
            capture_root(dispatcher.home()),
            // The clone cache retention: each capture publishes it as
            // `expiresAt`; the daily cache sweep removes captures older.
            dispatcher.limits().clone.ttl,
            dispatcher.paths(),
        )
    }

    fn at(
        root: PathBuf,
        ttl: Duration,
        paths: &'a PathPolicy,
    ) -> Result<Self, ClassificationError> {
        // The policy judges the directory itself, so it must exist: an
        // allowed root's missing parents are not inside that root.
        let outside = |_| {
            failure(
                "classificationContextUnavailable",
                "The capture directory is outside the local path policy.",
                "Supply the page text as state.",
            )
        };
        octocode_io::atomic::create_private_dir_all(&root).map_err(|_| {
            failure(
                "classificationContextUnavailable",
                "The capture directory could not be created.",
                "Check that the Octocode home is writable, or supply the text as state.",
            )
        })?;
        paths.validate(&root).map_err(outside)?;
        Ok(Self { root, ttl, paths })
    }

    /// Save the selection immutably and build its state, provenance and
    /// read lead.
    fn save(&self, selection: Selection) -> Result<Captured, ClassificationError> {
        let bytes = selection.text.as_bytes();
        let saved = self.save_bytes(bytes, CAPTURE_FILE)?;
        let snapshot = saved["snapshot"].as_str().unwrap_or_default();
        let file = saved["path"].as_str().unwrap_or_default();
        let lines = selection.text.lines().count();
        let url = selection.url.as_str();
        let read = crate::response::result::Continuation::new(
            ToolId::LocalFetch,
            json!({
                "path": file,
                "snapshot": snapshot,
                "ranges": [format!("1-{lines}")],
            }),
        )
        .build();
        Ok(Captured {
            state: json!({
                "source": { "url": url, "section": selection.section },
                "text": selection.text,
            }),
            read,
            source: Some(
                json!({ "url": url, "snapshot": snapshot, "expiresAt": saved["expiresAt"] }),
            ),
        })
    }

    /// Save exact source bytes without interpreting them as text. Images
    /// share the existing immutable capture storage, authorization and TTL.
    pub(super) fn save_bytes(
        &self,
        bytes: &[u8],
        name: &str,
    ) -> Result<serde_json::Value, ClassificationError> {
        let snapshot = octocode_engine::digest::sha256(bytes);
        let directory = self.root.join(&snapshot);
        let file = directory.join(name);
        let unsaved = |_| {
            failure(
                "classificationContextUnavailable",
                "The captured source could not be saved; nothing was judged.",
                "Check that the Octocode home is writable.",
            )
        };
        octocode_io::atomic::create_private_dir_all(&directory).map_err(unsaved)?;
        octocode_io::atomic::write_atomic(&file, bytes, false).map_err(unsaved)?;
        let readable = self.paths.validate_read(&file).map_err(|_| {
            failure(
                "classificationContextUnavailable",
                "The saved capture is not readable under the local path policy.",
                "Supply the page text as state.",
            )
        })?;
        let expires_at = crate::civil_date::iso8601_millis_after(self.ttl);
        Ok(json!({"path": readable.canonical, "snapshot": snapshot, "expiresAt": expires_at}))
    }
}

#[cfg(test)]
mod tests;
