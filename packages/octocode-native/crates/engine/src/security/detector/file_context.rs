//! What a pattern's file context means when the text is not one file's whole
//! content: which patterns only their context tells from other values, and
//! which need a whole file to be judged at all.
use super::{PatternSet, file_context_regex, gates, prescan_candidates};
use crate::security::patterns::PATTERNS;
use std::sync::LazyLock;

/// Index-aligned with PATTERNS: the pattern is a bare shape, no literal gates
/// it (see `gates::FALLBACK`). With a file context, that context is all that
/// tells a key from any other value of its shape: `wandbApiKey` is every
/// 40-hex string, so a commit SHA beside a path naming `wandb` is not one.
pub(super) static BARE_SHAPE: LazyLock<Vec<bool>> = LazyLock::new(|| {
    let mut bare = vec![false; PATTERNS.len()];
    for &idx in gates::FALLBACK {
        bare[idx] = true;
    }
    bare
});

/// The file-context patterns that span lines, so what makes a value a secret
/// can lie anywhere in its file (`kind: Secret` above a `data:` block). Only
/// a whole file can be judged for these.
static SPANS_LINES: LazyLock<PatternSet> = LazyLock::new(|| {
    let mut spanning = PatternSet::default();
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        if pattern.file_context.is_some() && pattern.regex.contains(r"[\s\S]") {
            spanning.insert(idx);
        }
    }
    spanning
});

/// What a source at one path can hold that no cut of it shows: the patterns of
/// [`SPANS_LINES`] that apply there.
#[derive(Clone, Copy)]
pub struct WholeSourcePatterns(PatternSet);

impl WholeSourcePatterns {
    /// `None` when no such pattern applies to a file at `path`, as for most.
    pub fn for_path(path: &str) -> Option<Self> {
        let applying = SPANS_LINES
            .filter(|idx| file_context_regex(idx).is_some_and(|context| context.is_match(path)));
        (!applying.is_empty()).then_some(Self(applying))
    }

    /// Whether `text`, any part of a source, holds a literal one of them
    /// requires. No literal spans lines, so a source none of whose lines holds
    /// one cannot match, and needs no whole-file scan.
    pub fn named_by(self, text: &str) -> bool {
        prescan_candidates(text).intersects(self.0)
    }
}
