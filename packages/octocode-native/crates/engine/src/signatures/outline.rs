//! The symbols outline of one file: tree-sitter declarations rendered as a
//! skeleton, or the typed reason a reader falls back to another view.
use super::extractor::LangExtractConfig;
use super::{MAX_PARSE_SIZE, extractor, languages, renderer};
use crate::text::extension_of;

/// Why a file has no symbols outline: the typed reason a reader falls back
/// to another view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutlineUnavailable {
    /// Larger than [`MAX_PARSE_SIZE`]: not parsed.
    TooLarge,
    /// No grammar with an outline for this file type.
    UnsupportedLanguage,
    /// The parser gave no tree in time.
    ParseFailed,
    /// The parse kept no declaration line.
    NoDeclarations,
    /// The outline would be no smaller than the source.
    NotSmaller,
    /// Over-long lines (minified or bundled source) make up most of the
    /// outline, so it is not a scannable list of signatures.
    Minified,
}

impl OutlineUnavailable {
    /// The reason as the readers report it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "too-large",
            Self::UnsupportedLanguage => "unsupported-language",
            Self::ParseFailed => "parse-failed",
            Self::NoDeclarations => "no-declarations",
            Self::NotSmaller => "not-smaller",
            Self::Minified => "minified-lines",
        }
    }
}

/// Extract a structural skeleton from `content`: `N\ttext` lines, or the
/// typed reason there is none.
pub(crate) fn extract_signatures_inner(
    content: &str,
    file_path: &str,
) -> Result<String, OutlineUnavailable> {
    if content.len() > MAX_PARSE_SIZE {
        return Err(OutlineUnavailable::TooLarge);
    }
    let skeleton = std::panic::catch_unwind(|| {
        let ext = extension_of(file_path, true, "txt");
        extract_by_ext(
            content,
            crate::text::file_extension::grammar_extension(content, &ext),
        )
    })
    .unwrap_or(Err(OutlineUnavailable::ParseFailed))?;
    usable_outline(content, skeleton)
}

/// `skeleton` as the outline of `content`, unless it is no smaller than the
/// source or mostly over-long lines.
pub(super) fn usable_outline(
    content: &str,
    skeleton: String,
) -> Result<String, OutlineUnavailable> {
    // Return the source view when the rendered outline is no smaller. A file
    // with few body lines can grow once the outline includes line gutters.
    //
    // The comparison is on the rendered output the agent actually receives
    // (gutter included) — that is the byte count we promise never to inflate.
    // Tiny files where the per-line gutter alone tips the balance are suppressed
    // too, which is correct: a symbol outline of a handful of lines carries no
    // navigational value over just showing the lines.
    if skeleton.len() >= content.len() {
        return Err(OutlineUnavailable::NotSmaller);
    }

    // A usable outline is scannable: mostly one short signature per line.
    // Minified or bundled sources pack whole declarations onto a few enormous
    // physical lines, so per-line extraction keeps those lines verbatim and
    // the "outline" is really raw minified bytes. That holds when over-long
    // lines make up most of the outline; a few long declarations (generated
    // one-line structs) in an otherwise normal outline stay, verbatim, and
    // their range gutter makes each body one `ranges` read away.
    let long_chars: usize = skeleton
        .lines()
        .map(|line| line.chars().count())
        .filter(|chars| *chars > OUTLINE_MAX_LINE_CHARS)
        .sum();
    if long_chars.saturating_mul(2) > skeleton.chars().count() {
        return Err(OutlineUnavailable::Minified);
    }
    Ok(skeleton)
}

/// Whether `line` is a whole-line comment in the comment syntax of the
/// grammar that reads `file_path`; `None` when no grammar reads it.
#[must_use]
pub fn is_comment_line(file_path: &str, line: &str) -> Option<bool> {
    let ext = extension_of(file_path, true, "txt");
    let entry = languages::find_entry(&ext)?;
    Some(renderer::is_pure_comment(line, entry.comment_style))
}

/// Longest rendered outline line (gutter included) still treated as a scannable
/// signature. Hand-written declaration lines effectively never reach this;
/// an outline made mostly of longer lines is minified/bundled source.
pub(super) const OUTLINE_MAX_LINE_CHARS: usize = 1000;

fn extract_by_ext(content: &str, ext: &str) -> Result<String, OutlineUnavailable> {
    // Tree-sitter is the only signature path — real AST parsing, no regex
    // heuristics. Languages outside the canonical registry have no outline
    // and the caller falls back to the standard/none view.
    let entry = languages::find_entry(ext)
        .filter(|entry| entry.admits(crate::text::analysis::AnalysisOperation::SignatureOutline))
        .ok_or(OutlineUnavailable::UnsupportedLanguage)?;
    let cfg = LangExtractConfig {
        language: entry.language.clone(),
        body_query: entry.body_query,
    };
    let kept = extractor::extract_outline(content, &cfg).ok_or(OutlineUnavailable::ParseFailed)?;
    renderer::render_skeleton(&kept, entry.comment_style).ok_or(OutlineUnavailable::NoDeclarations)
}
