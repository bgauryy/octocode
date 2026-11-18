//! GitHub file bytes as text by the local tools' one source admission
//! (`octocode_engine::text::admit`): the provider returns a file's bytes,
//! and this rule, not the provider, decides whether they are text.
use octocode_engine::text::{Decoding, admit};
use octocode_github::{ContentResponse, ProviderError, ProviderErrorKind, ProviderErrorReason};
use std::borrow::Cow;

/// The text of a file's bytes (UTF-8, BOM-marked UTF-16, else Latin-1 or
/// lossy UTF-8, as localFetch shows them), or the typed binary-file failure
/// every GitHub read states for bytes the rule refuses.
pub fn file_text(bytes: &[u8]) -> Result<Cow<'_, str>, ProviderError> {
    admit(bytes, Decoding::Lenient)
        .map(|decoded| decoded.text)
        .map_err(|_| binary_file(bytes.len(), None))
}

/// A file read the rule admits as text; refused, the typed binary-file
/// failure names the read's blob SHA when the provider learned it.
pub fn admit_read(read: &ContentResponse) -> Result<(), ProviderError> {
    match admit(&read.bytes, Decoding::Lenient) {
        Ok(_) => Ok(()),
        Err(_) => Err(binary_file(read.bytes.len(), read.blob.as_deref())),
    }
}

/// `Binary file (N bytes[, blob SHA])`: a request a text read cannot serve,
/// not a decode fault.
fn binary_file(len: usize, blob: Option<&str>) -> ProviderError {
    let blob = blob.map(|sha| format!(", blob {sha}")).unwrap_or_default();
    ProviderError::new(
        ProviderErrorKind::Validation,
        format!("Binary file ({len} bytes{blob})"),
    )
    .with_reason(ProviderErrorReason::BinaryFile)
}

#[cfg(test)]
mod tests;
