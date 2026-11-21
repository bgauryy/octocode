//! What ranking needs of a provider text match, and none of its text. A
//! fragment's edges are the index's, not a line's or a token's: a secret
//! they cut can lose the prefix that names it, so no fragment text leaves
//! here. Rows list sealed blob lines instead.
use super::ranking::Term;
use octocode_github::{ProviderError, TextMatch};
use octocode_runtime_base::security::scan::{ContentScan, sanitize_provider_text};
use std::path::Path;

/// The score of one fragment of `path`: its sanitized text against `terms`;
/// `None` when sanitizing leaves no text. The scanner's refusal is the error.
pub(super) fn score(
    fragment: &TextMatch,
    path: &str,
    terms: &[Term],
    security: &impl ContentScan,
) -> Result<Option<u8>, ProviderError> {
    let sanitized = sanitize_provider_text(security, &fragment.fragment, Path::new(path))?;
    if sanitized.is_empty() {
        return Ok(None);
    }
    Ok(Some(super::ranking::score(path, &sanitized, terms)))
}

#[cfg(test)]
mod tests;
