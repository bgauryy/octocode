//! What a pattern's file context means when the text is not one file's whole
//! content: which patterns only their context tells from other values, and
//! which need a whole file to be judged at all.
use super::document::Form;
use super::{PatternSet, file_context_regex, gates, jwk, pattern_regex_for, prescan_candidates};
use crate::security::patterns::PATTERNS;
use regex::Regex;
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

/// The patterns of `form`.
fn patterns_of(forms: &[Form]) -> PatternSet {
    let mut set = PatternSet::default();
    for idx in 0..PATTERNS.len() {
        if Form::of(idx).is_some_and(|form| forms.contains(&form)) {
            set.insert(idx);
        }
    }
    set
}

/// The file-context patterns that span lines, so what makes a value a secret
/// can lie anywhere in its file (`kind: Secret` above a `data:` block). Only
/// a whole file can be judged for these.
static SPANS_LINES: LazyLock<PatternSet> = LazyLock::new(|| patterns_of(&[Form::SecretInYaml]));

/// The patterns of any file whose value can sit lines from what makes it secret,
/// though a read cut around it still holds both or can be judged line by line.
static SPANS_OBJECT: LazyLock<PatternSet> =
    LazyLock::new(|| patterns_of(&[Form::SecretInJson, Form::JsonWebKey]));

/// A JSON member holding a string, as a Secret's value is, whatever it spells:
/// `stringData` holds plain words.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static STRING_MEMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""[^"\r\n]+"\s*:\s*\\*""#).expect("static pattern compiles"));

/// What a source at one path can hold that no cut of it shows: the patterns of
/// [`SPANS_LINES`] that apply there.
#[derive(Clone, Copy)]
pub struct WholeSourcePatterns(PatternSet);

impl WholeSourcePatterns {
    /// What every text [`Self::any_file`]'s patterns apply to holds: a text with
    /// neither needs no regex to rule them out.
    const OBJECT_LITERALS: [&'static str; 2] = ["kty", "\"Secret"];

    /// `None` when no such pattern applies to a file at `path`, as for most.
    pub fn for_path(path: &str) -> Option<Self> {
        let applying = SPANS_LINES
            .filter(|idx| file_context_regex(idx).is_some_and(|context| context.is_match(path)));
        (!applying.is_empty()).then_some(Self(applying))
    }

    /// What any source can hold that a line of it does not show, for a caller
    /// with the whole source in hand.
    pub fn any_file() -> Self {
        Self(*SPANS_OBJECT)
    }

    /// These patterns and `other`'s.
    #[must_use]
    pub fn and(self, other: Option<Self>) -> Self {
        let mut joined = self.0;
        if let Some(other) = other {
            for idx in other.0.iter() {
                joined.insert(idx);
            }
        }
        Self(joined)
    }

    /// [`Self::any_file`] when `line` shows a value only the rest of its source
    /// can judge. Most lines show none, so most reads stop at their window.
    pub fn shown_by(line: &str) -> Option<Self> {
        (jwk::shows_member(line) || STRING_MEMBER.is_match(line)).then(Self::any_file)
    }

    /// Whether a whole read of `text` cannot rely on its pages' margins: a JSON
    /// Web Key's members are as far apart as its certificate chain is long, and a
    /// Secret's kind as far from its data as its `metadata` is. Nor can such a
    /// text be judged line by line when it overruns the scanner.
    pub fn outruns_a_page(text: &str) -> bool {
        Self::OBJECT_LITERALS
            .iter()
            .any(|literal| text.contains(literal))
            && SPANS_OBJECT
                .iter()
                .any(|idx| pattern_regex_for(idx, text).is_match(text))
    }

    /// Whether `text`, any part of a source, holds a literal one of them
    /// requires. No literal spans lines, so a source none of whose lines holds
    /// one cannot match, and needs no whole-file scan.
    pub fn named_by(self, text: &str) -> bool {
        prescan_candidates(text).intersects(self.0)
    }
}
