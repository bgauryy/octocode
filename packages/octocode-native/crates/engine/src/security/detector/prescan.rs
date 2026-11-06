//! The literal prescan: the cheap gate that picks a text's candidate
//! patterns before any pattern regex compiles.

use super::super::patterns::PATTERN_COUNT;
use super::gates;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Literal prescan — the cheap gate in front of regex compilation.
//
// The old path lazily built a 309-branch RegexSet DFA plus 309 individual
// regexes on the FIRST sanitize call (~250-800 ms), a tax every fresh process
// paid even for entirely clean content. The prescan replaces it:
//
//   1. At build time (`build.rs`), extract every pattern's gate via
//      regex-syntax HIR: its *required prefix literals*, else literals its
//      structure requires inside every match. A pattern gates on its
//      literals only when extraction proves the set is finite and every
//      literal is a meaningful anchor (>= 3 bytes).
//   2. At first call, build ONE case-insensitive aho-corasick automaton over
//      those literals.
//   3. Per call: scan content once with aho-corasick; only patterns whose
//      literal appears (plus the small no-literal fallback bucket) get their
//      regex compiled (lazily, once per process) and confirmed with is_match.
//
// Correctness invariant: for a gated pattern, "regex matches content" implies
// "content contains one of its gate literals" (a match begins with a prefix
// literal, and contains a match of every concatenated part, of one branch of
// an alternation, and of a repetition's body when its minimum is one), and
// patterns are parsed with their flags, so a `(?i)` pattern's literals carry
// every Unicode case variant its regex matches (`ſ` for `s`); the automaton
// is ASCII-case-insensitive over literals stored lowercased. Patterns whose
// literals cannot be proven (pure entropy shapes, UUIDs) are ALWAYS
// candidates — they are never gated out.
// ---------------------------------------------------------------------------

/// One automaton over every gated pattern's literals; its pattern id indexes
/// `gates::GATE_LITERALS`.
static PRESCAN: LazyLock<AhoCorasick> = LazyLock::new(|| {
    // Aho-Corasick over a fixed literal set cannot fail to build.
    #[expect(
        clippy::expect_used,
        reason = "built from static patterns, valid by construction"
    )]
    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(gates::GATE_LITERALS.iter().map(|(literal, _)| literal))
        .expect("prescan literal automaton must build")
});

/// A set of pattern indices, one bit per pattern, kept on the stack: picking
/// a text's candidates allocates nothing.
#[derive(Clone, Copy, Default)]
pub(super) struct PatternSet([u64; PATTERN_COUNT.div_ceil(64)]);

impl PatternSet {
    pub(super) fn insert(&mut self, idx: usize) {
        self.0[idx / 64] |= 1 << (idx % 64);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    pub(super) fn intersects(self, other: Self) -> bool {
        self.0.iter().zip(other.0).any(|(a, b)| a & b != 0)
    }

    /// The members, in canonical pattern order.
    pub(super) fn iter(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(at, word)| {
            let mut rest = word;
            std::iter::from_fn(move || {
                (rest != 0).then(|| {
                    let bit = rest.trailing_zeros() as usize;
                    rest &= rest - 1;
                    at * 64 + bit
                })
            })
        })
    }

    /// The members `keep` accepts.
    pub(super) fn filter(self, mut keep: impl FnMut(usize) -> bool) -> Self {
        let mut kept = Self::default();
        for idx in self.iter().filter(|&idx| keep(idx)) {
            kept.insert(idx);
        }
        kept
    }
}

/// Candidate patterns for `content`: gated patterns whose literal appears,
/// plus the always-on fallback bucket. Callers confirm each with the real
/// pattern regex (compiled lazily, only for candidates), so they still see
/// exact match semantics.
pub(super) fn prescan_candidates(content: &str) -> PatternSet {
    let mut candidates = PatternSet::default();
    // Overlapping scan: a literal nested inside another match span must still
    // register its own pattern, or that pattern would be silently gated out.
    for hit in PRESCAN.find_overlapping_iter(content) {
        candidates.insert(gates::GATE_LITERALS[hit.pattern().as_usize()].1);
    }
    for &idx in gates::FALLBACK {
        candidates.insert(idx);
    }
    candidates
}
