//! The reply decoded to page text, and why a page yields none.
use super::{Reply, extract, stopped};
use crate::tools::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;

/// The reply's media type and UTF-8 body. Only HTML, plain text and
/// Markdown are captured; anything else (binary documents, other charsets)
/// is unsupported, never judged from a lossy decoding.
pub(super) fn decode(reply: &Reply) -> Result<(String, String), ClassificationError> {
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
    // The engine's one source admission: binary refused, a leading BOM
    // dropped, and only Unicode text decoded.
    match octocode_engine::text::admit(&reply.body, octocode_engine::text::Decoding::Unicode) {
        Ok(decoded) => Ok((media, decoded.text.into_owned())),
        Err(octocode_engine::text::Refusal::Binary) => Err(unsupported("The page body is binary.")),
        Err(octocode_engine::text::Refusal::NotUnicode) => {
            Err(unsupported("The page body is not valid UTF-8."))
        }
    }
}

fn unsupported(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        message,
        "Capture an HTML or text page, or supply the content as content.",
    )
}

pub(super) fn extraction_failure(
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
        extract::Failure::Empty { scripted: true } => ClassificationError::new(
            ErrorCode::ClassificationContextUnavailable,
            "The page has no static text: it builds its content with scripts.",
            "Read the rendered page in a browser and supply its text as content.",
        ),
        extract::Failure::Empty { scripted: false } => ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            "The page has no text; nothing was judged.",
            "Check the URL.",
        ),
        extract::Failure::MissingAnchor => ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            format!("No element on the page is anchored #{fragment}; nothing was judged."),
            "Use an anchor the page defines, or drop the fragment to capture the whole page.",
        ),
        extract::Failure::AmbiguousAnchor(count) => ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            format!("{count} elements on the page are anchored #{fragment}; nothing was judged."),
            "Use an anchor the page defines once, or drop the fragment.",
        ),
        extract::Failure::EmptySection => ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            format!("The section anchored #{fragment} has no text; nothing was judged."),
            "Use another anchor, or drop the fragment to capture the whole page.",
        ),
    }
}

/// A percent-decoded fragment; the raw fragment when it does not decode.
pub(super) fn decode_fragment(raw: &str) -> String {
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
