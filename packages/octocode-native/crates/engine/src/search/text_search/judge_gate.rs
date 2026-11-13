//! When a file's own bytes prove that the text its judge serves holds no
//! match, so the judge need not run ([`crate::types::SourceJudge`]).
//!
//! A judge serves a file's text with some runs of its bytes replaced by text
//! it writes ([`crate::types::SourceJudge::writes`]). A match in the served
//! text that takes none of that written text is the source's own bytes at
//! their place, so the source holds it too. Every match begins with one of the
//! pattern's prefix literals ([`regex_syntax::hir::literal`]); when none of
//! them can take written text, a source without any of them serves a text
//! without a match. A pattern whose matches are exactly its literals, with no
//! assertion about what is around them, needs no prefix scan: the search of
//! the source's own bytes finding no line is the proof.
use crate::types::{CaseRule, PatternSyntax, SearchOutput, TextSearchOptions};
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use std::io::Read as _;

/// How a search decides, per file, that its judge need not run.
pub(super) enum JudgeGate {
    /// Every file is judged: the search has no judge, or a match could take
    /// text the judge writes, or a file without a match is shown.
    Always,
    /// A file the search of whose own bytes found no line is not judged when
    /// `exact` holds (and the search read it to its end), or when its bytes
    /// before the first NUL hold none of `prefixes`.
    Gated {
        /// Every match is one of the literals, whatever is around it.
        exact: bool,
        /// The prefix literals, compared as the search compares case.
        prefixes: regex::bytes::Regex,
        /// Bytes a prefix match may take: the scan's overlap between reads.
        reach: usize,
    },
}

/// Bytes each read of the prefix scan takes.
const SCAN_BYTES: usize = 64 * 1024;

impl JudgeGate {
    /// The gate of a search of `opts`, whose pattern the Rust engine reads
    /// as `spelled`.
    pub(super) fn of(opts: &TextSearchOptions, spelled: &str) -> Self {
        Self::gated(opts, spelled).unwrap_or(Self::Always)
    }

    fn gated(opts: &TextSearchOptions, spelled: &str) -> Option<Self> {
        let judge = opts.judge.as_ref()?;
        // A files-without view or an inverted search shows the files and
        // lines that do not match: their text is the judge's.
        if opts.invert_match.unwrap_or(false)
            || opts.output == SearchOutput::FilesWithoutMatch
            || opts.syntax == PatternSyntax::Pcre2
        {
            return None;
        }
        let multiline = opts.multiline.unwrap_or(false);
        let hir = regex_syntax::ParserBuilder::new()
            .multi_line(true)
            .dot_matches_new_line(multiline && opts.multiline_dotall.unwrap_or(false))
            .utf8(false)
            .build()
            .parse(spelled)
            .ok()?;
        let seq = Extractor::new().kind(ExtractKind::Prefix).extract(&hir);
        let literals = seq.literals()?;
        // A smart-case search compares without case when its pattern has no
        // capital; comparing without case here only judges more.
        let fold = opts.case != CaseRule::Sensitive;
        let mut alternates = Vec::with_capacity(literals.len());
        let mut reach = 0;
        for literal in literals {
            let text = std::str::from_utf8(literal.as_bytes()).ok()?;
            if text.is_empty() || (judge.writes)(text, fold) {
                return None;
            }
            // A character compared without case may take up to four bytes.
            reach = reach.max(text.chars().count() * 4);
            alternates.push(regex::escape(text));
        }
        let prefixes = regex::bytes::RegexBuilder::new(&alternates.join("|"))
            .case_insensitive(fold)
            .build()
            .ok()?;
        let exact = seq.is_exact()
            && hir.properties().look_set().is_empty()
            && !opts.whole_word.unwrap_or(false);
        Some(Self::Gated {
            exact,
            prefixes,
            reach,
        })
    }

    /// Whether the search of `file`'s own bytes, which found `matched` lines
    /// and quit at a NUL when `binary`, proves the text its judge serves
    /// holds no match. `file` is read from its start.
    pub(super) fn proves_unmatched(
        &self,
        file: &std::fs::File,
        matched: u32,
        binary: bool,
    ) -> std::io::Result<bool> {
        let Self::Gated {
            exact,
            prefixes,
            reach,
        } = self
        else {
            return Ok(false);
        };
        if matched > 0 {
            return Ok(false);
        }
        if *exact && !binary {
            return Ok(true);
        }
        absent_before_nul(file, prefixes, *reach)
    }

    /// Whether some file of the search may skip its judge.
    pub(super) fn gates(&self) -> bool {
        matches!(self, Self::Gated { .. })
    }
}

/// Whether `file`'s bytes before its first NUL hold no match of `prefixes`.
/// A judge writes only before the first NUL, and a search reports no hit
/// past it. A UTF-16 source is searched as its decoded text, which its bytes
/// do not hold: it is never proved.
fn absent_before_nul(
    file: &std::fs::File,
    prefixes: &regex::bytes::Regex,
    reach: usize,
) -> std::io::Result<bool> {
    use std::io::{Seek, SeekFrom};
    let mut handle = file;
    handle.seek(SeekFrom::Start(0))?;
    let mut window: Vec<u8> = Vec::with_capacity(SCAN_BYTES + reach);
    let mut chunk = vec![0u8; SCAN_BYTES];
    let mut first = true;
    loop {
        let read = match handle.read(&mut chunk) {
            Ok(0) => return Ok(true),
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let bytes = &chunk[..read];
        if first {
            first = false;
            if crate::text::has_utf16_bom(bytes) {
                return Ok(false);
            }
        }
        let nul = memchr::memchr(0, bytes);
        window.extend_from_slice(&bytes[..nul.unwrap_or(read)]);
        if prefixes.is_match(&window) {
            return Ok(false);
        }
        if nul.is_some() {
            return Ok(true);
        }
        let keep = window.len().min(reach);
        window.drain(..window.len() - keep);
    }
}
