//! The reply decoded to page text, and why a page yields none.
use super::{Reply, extract, stopped};
use crate::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;

/// Whether `media` is text whatever its name says: any `text/*`, JSON, XML,
/// JavaScript and their `+json`/`+xml` kin. A body that is not text is still
/// refused below, by its bytes.
fn is_text(media: &str) -> bool {
    media.starts_with("text/")
        || media.ends_with("+json")
        || media.ends_with("+xml")
        || matches!(
            media,
            "application/json"
                | "application/javascript"
                | "application/x-javascript"
                | "application/ecmascript"
                | "application/xml"
                | "application/yaml"
                | "application/toml"
        )
}

/// The reply's media type and body as text. Text in any declared charset is
/// captured; anything else (binary documents) is unsupported, never judged
/// from a lossy decoding.
pub(super) fn decode(reply: &Reply) -> Result<(String, String), ClassificationError> {
    let content_type = reply.content_type.as_deref().unwrap_or_default();
    let mut parameters = content_type.split(';');
    let media = parameters
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !media.is_empty() && !is_text(&media) {
        return Err(unsupported(&format!("The page is {media}, not text.")));
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
        let decoded = encoding_rs::Encoding::for_label(charset.as_bytes())
            .and_then(|encoding| {
                encoding.decode_without_bom_handling_and_without_replacement(&reply.body)
            })
            .ok_or_else(|| {
                unsupported(&format!(
                    "The page declares charset {charset}, which does not decode it."
                ))
            })?;
        return Ok((media, decoded.into_owned()));
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
        "This reader takes text; supply it as content, or as an image under assets.",
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
            .unwrap_or_else(|| stopped(octocode_runtime_base::execution::ExecutionError::Timeout)),
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
