//! What a source too large to judge whole names that only its structure can
//! judge, and that structure, walked as the source passes.
//!
//! [`SourceNames`] is the cheap check a read pays on every byte: it looks at the
//! bytes around each literal, so `kty` inside a base64 blob names nothing, and only
//! a spelling the walks would act on counts. It is wider than the walks and no
//! narrower: whatever it leaves out, they would have read past. [`SourceWalk`] then
//! says what is secret, to the byte, within a memory budget.
use super::{WholeSourcePatterns, objects, yaml_secret};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use std::ops::Range;
use std::sync::LazyLock;

/// Bytes looked at on either side of a literal. Blanks that outrun them make the
/// spelling count: the walk then decides.
const REACH: usize = 64;
/// The bytes kept between pieces: a literal the last piece could not judge, with
/// [`REACH`] on both sides of it.
const KEPT: usize = 2 * REACH + 8;

const KEY_TYPE: &[u8] = b"kty";
const SECRET: &[u8] = b"\"Secret";
const AUTHS: &[u8] = b"\"auths";
const KIND_LINE: &[u8] = b"kind:";

#[expect(clippy::expect_used, reason = "built from static literals")]
static OBJECT_LITERALS: LazyLock<AhoCorasick> =
    LazyLock::new(|| AhoCorasick::new([KEY_TYPE, SECRET]).expect("literal automaton"));

/// The object literals matched in any ASCII case: Go's `encoding/json` reads a
/// Docker config's `"Auths"` as its `"auths"`.
#[expect(clippy::expect_used, reason = "built from static literals")]
static FOLDED_OBJECT_LITERALS: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build([AUTHS])
        .expect("literal automaton")
});

#[expect(clippy::expect_used, reason = "built from static literals")]
static KIND_LINES: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build([KIND_LINE])
        .expect("literal automaton")
});

/// Which families of objects a source names: each is what the walk of its
/// objects finds under one spelling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Objects {
    /// A `"kind": "Secret"`: a Kubernetes Secret's values.
    pub secret: bool,
    /// A `kty`: a JSON Web Key's private members.
    pub key: bool,
    /// An `"auths"` key: a Docker config's registry credentials.
    pub registry: bool,
}

impl Objects {
    /// Every family: a source whose walk reads them all, whatever it names.
    pub const ALL: Self = Self {
        secret: true,
        key: true,
        registry: true,
    };

    /// Whether any family is named.
    #[must_use]
    pub const fn any(self) -> bool {
        self.secret || self.key || self.registry
    }

    /// The pattern a value of these objects is hidden under when no walk placed
    /// it: the form of the one family named, and when several are named the
    /// Secret's, the pattern of a value with no family of its own.
    #[must_use]
    pub const fn form(self) -> SecretForm {
        match (self.secret, self.key, self.registry) {
            (false, true, false) => SecretForm::JsonWebKey,
            (false, false, true) => SecretForm::DockerAuths,
            _ => SecretForm::SecretInJson,
        }
    }
}

/// What a source names, and so which walk could find something in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Named {
    /// The families of objects it names: the walk of its objects.
    pub objects: Objects,
    /// `kind: Secret` at a path whose YAML is judged: the walk of its mappings.
    pub yaml: bool,
}

impl Named {
    #[must_use]
    pub fn any(self) -> bool {
        self.objects.any() || self.yaml
    }
}

/// The literal check, fed every byte of a source once.
pub struct SourceNames {
    named: Named,
    /// Whether this source's YAML is judged.
    yaml: bool,
    /// The last bytes of what came before.
    tail: Vec<u8>,
    /// `tail` starts at the source's first byte.
    from_start: bool,
}

impl SourceNames {
    /// The check of the source at `path`.
    #[must_use]
    pub fn for_path(path: &str) -> Self {
        Self {
            named: Named::default(),
            yaml: WholeSourcePatterns::for_path(path).is_some(),
            tail: Vec::with_capacity(2 * KEPT),
            from_start: true,
        }
    }

    fn settled(&self) -> bool {
        self.named.objects == Objects::ALL && (self.named.yaml || !self.yaml)
    }

    /// Take the source's next `bytes`.
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.settled() {
            return;
        }
        // Across the seam with the last piece, and through the bytes themselves
        // from where each literal has its reach behind it inside them.
        let seam = bytes.len().min(KEPT);
        self.tail.extend_from_slice(&bytes[..seam]);
        let tail = std::mem::take(&mut self.tail);
        self.judge(&tail, self.from_start, false);
        self.tail = tail;
        if bytes.len() > seam {
            self.judge(bytes, false, false);
            self.tail.clear();
            self.tail
                .extend_from_slice(&bytes[bytes.len().saturating_sub(KEPT)..]);
            self.from_start = false;
        } else if self.tail.len() > KEPT {
            self.tail.drain(..self.tail.len() - KEPT);
            self.from_start = false;
        }
    }

    /// What the whole source named.
    #[must_use]
    pub fn finish(mut self) -> Named {
        let tail = std::mem::take(&mut self.tail);
        self.judge(&tail, self.from_start, true);
        self.named
    }

    fn judge(&mut self, text: &[u8], from_start: bool, last: bool) {
        // Away from a start, a literal sooner than its reach was judged in the seam.
        let usable = |at: usize| from_start || at >= REACH;
        let objects = &mut self.named.objects;
        if !(objects.key && objects.secret) {
            for hit in OBJECT_LITERALS.find_iter(text) {
                if !usable(hit.start()) {
                    continue;
                }
                if hit.end() - hit.start() == KEY_TYPE.len() {
                    objects.key = objects.key || names_a_key_type(text, hit.start(), last);
                } else {
                    objects.secret =
                        objects.secret || names_a_secret(text, hit.start(), from_start);
                }
            }
        }
        if !objects.registry {
            objects.registry = FOLDED_OBJECT_LITERALS
                .find_iter(text)
                .any(|hit| usable(hit.start()) && names_registry_auths(text, hit.start(), last));
        }
        if self.yaml && !self.named.yaml {
            self.named.yaml = KIND_LINES
                .find_iter(text)
                .any(|hit| usable(hit.start()) && names_a_yaml_secret(text, hit.start(), last));
        }
    }
}

/// `byte` continues a bare name in [`objects`].
fn continues_a_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'$') || byte >= 0x80
}

fn after_blanks(text: &[u8], blanks: impl Fn(u8) -> bool) -> &[u8] {
    let skipped = text.iter().take_while(|byte| blanks(**byte)).count();
    &text[skipped..]
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// Whether `reach` is all a literal's reach: running out of it with nothing
/// decided, the spelling counts and the walk decides. Short of that, the bytes
/// ran out, so the feed that brings more judges it again from the kept tail.
fn outran(reach: &[u8]) -> bool {
    reach.len() >= REACH
}

/// `kty` at `at` is a member's name: not the end of a longer word, and a colon
/// after its closing quote and blanks. `Kty::Rsa` and base64 hold neither.
fn names_a_key_type(text: &[u8], at: usize, last: bool) -> bool {
    if at > 0 && continues_a_name(text[at - 1]) {
        return false;
    }
    let reach = &text[at + KEY_TYPE.len()..(at + KEY_TYPE.len() + REACH).min(text.len())];
    let mut rest = after_blanks(reach, |byte| byte == b'\\');
    if let Some((b'"' | b'\'', quoted)) = rest.split_first() {
        rest = quoted;
    }
    match after_blanks(rest, is_space) {
        [] => outran(reach),
        [b':'] => outran(reach) || last,
        [b':', b':', ..] => false,
        [b':', ..] => true,
        _ => false,
    }
}

/// `"auths` at `at`, in any case, is a member's name: its string closes, quotes
/// held at any depth, and a colon follows. A Docker client config's `auths` map
/// has it.
fn names_registry_auths(text: &[u8], at: usize, last: bool) -> bool {
    let reach = &text[at + AUTHS.len()..(at + AUTHS.len() + REACH).min(text.len())];
    let rest = after_blanks(reach, |byte| byte == b'\\');
    let rest = match rest.split_first() {
        Some((b'"', closed)) => closed,
        Some(_) => return false,
        None => return outran(reach),
    };
    match after_blanks(rest, is_space) {
        [] => outran(reach) || last,
        [b':', ..] => true,
        _ => false,
    }
}

/// `"Secret` at `at` is the value of a `kind` key, whose string ends there.
fn names_a_secret(text: &[u8], at: usize, from_start: bool) -> bool {
    let reach = &text[at + SECRET.len()..(at + SECRET.len() + REACH).min(text.len())];
    match after_blanks(reach, |byte| byte == b'\\') {
        [] if !outran(reach) => return false,
        [] | [b'"', ..] => {}
        _ => return false,
    }
    kind_key_before(
        &text[at.saturating_sub(REACH)..at],
        from_start && at <= REACH,
    )
}

/// `before` ends in `"kind":`, quotes held at any depth, with any blanks. When the
/// bytes run out, the walk decides, unless they ran out at the source's start.
fn kind_key_before(before: &[u8], at_start: bool) -> bool {
    let ran_out = !at_start;
    let strip_back = |text: &'_ [u8], drop: fn(u8) -> bool| -> usize {
        text.len() - text.iter().rev().take_while(|byte| drop(**byte)).count()
    };
    let mut text = before;
    text = &text[..strip_back(text, |byte| byte == b'\\')];
    text = &text[..strip_back(text, is_space)];
    let Some(rest) = text.strip_suffix(b":") else {
        return text.is_empty() && ran_out;
    };
    text = &rest[..strip_back(rest, is_space)];
    let Some(rest) = text.strip_suffix(b"\"") else {
        return text.is_empty() && ran_out;
    };
    text = &rest[..strip_back(rest, |byte| byte == b'\\')];
    let Some(rest) = text.strip_suffix(b"kind") else {
        return b"kind".ends_with(text) && ran_out;
    };
    match rest.last() {
        Some(b'"') => true,
        Some(_) => false,
        None => ran_out,
    }
}

/// `kind:` at `at` starts a word, and `Secret` follows it, ending a word.
fn names_a_yaml_secret(text: &[u8], at: usize, last: bool) -> bool {
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80;
    if at > 0 && is_word(text[at - 1]) {
        return false;
    }
    let reach = &text[at + KIND_LINE.len()..(at + KIND_LINE.len() + REACH).min(text.len())];
    let mut rest = after_blanks(reach, |byte| matches!(byte, b' ' | b'\t'));
    if let Some((b'"' | b'\'', quoted)) = rest.split_first() {
        rest = quoted;
    }
    let spelled = b"secret";
    if rest.len() < spelled.len() {
        return outran(reach);
    }
    rest[..spelled.len()].eq_ignore_ascii_case(spelled)
        && rest
            .get(spelled.len())
            .map_or(last || outran(reach), |byte| !is_word(*byte))
}

/// Which pattern a span was hidden under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretForm {
    SecretInJson,
    JsonWebKey,
    SecretInYaml,
    DockerAuths,
}

impl SecretForm {
    /// The pattern's name, as its placeholder spells it.
    #[must_use]
    pub const fn pattern(self) -> &'static str {
        match self {
            Self::SecretInJson => "kubernetesSecretJson",
            Self::JsonWebKey => super::jwk::PATTERN,
            Self::SecretInYaml => "kubernetesSecrets",
            Self::DockerAuths => super::document::DOCKER_AUTHS,
        }
    }
}

/// Bytes of a source that are secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretSpan {
    pub range: Range<usize>,
    pub form: SecretForm,
}

/// What a walk of a whole source found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretSpans {
    /// These bytes, in order, none overlapping: all the source holds.
    Exact(Vec<SecretSpan>),
    /// More than the walk could keep: no span is known, so none can be trusted.
    TooMany,
}

/// The walks of a source, fed every byte once.
pub struct SourceWalk {
    objects: Option<objects::Walk>,
    yaml: Option<yaml_secret::Blocks>,
    /// Bytes the walks may hold between them.
    budget: usize,
    over: bool,
}

impl SourceWalk {
    #[must_use]
    pub fn new(named: Named, budget: usize) -> Self {
        Self {
            objects: named.objects.any().then(|| objects::Walk::within(budget)),
            yaml: named.yaml.then(yaml_secret::Blocks::default),
            budget,
            over: false,
        }
    }

    /// Take the source's next `bytes`.
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.over {
            return;
        }
        if let Some(walk) = &mut self.objects {
            walk.feed(bytes);
        }
        if let Some(blocks) = &mut self.yaml {
            blocks.feed(bytes);
        }
        let weight = self.objects.as_ref().map_or(0, objects::Walk::weight)
            + self.yaml.as_ref().map_or(0, yaml_secret::Blocks::weight);
        if weight > self.budget {
            self.over = true;
            if let Some(walk) = &mut self.objects {
                walk.give_up();
            }
            if let Some(blocks) = &mut self.yaml {
                blocks.give_up();
            }
        }
    }

    /// What the whole source holds.
    #[must_use]
    pub fn finish(self) -> SecretSpans {
        let mut spans = Vec::new();
        if let Some(walk) = self.objects {
            let Some(found) = walk.finish() else {
                return SecretSpans::TooMany;
            };
            let spanned = |form: SecretForm| move |range| SecretSpan { range, form };
            spans.extend(
                found
                    .secret_values
                    .into_iter()
                    .map(spanned(SecretForm::SecretInJson)),
            );
            spans.extend(
                found
                    .private_members
                    .into_iter()
                    .map(spanned(SecretForm::JsonWebKey)),
            );
            spans.extend(
                found
                    .registry_credentials
                    .into_iter()
                    .map(spanned(SecretForm::DockerAuths)),
            );
        }
        if let Some(blocks) = self.yaml {
            let Some(found) = blocks.finish() else {
                return SecretSpans::TooMany;
            };
            spans.extend(found.into_iter().map(|range| SecretSpan {
                range,
                form: SecretForm::SecretInYaml,
            }));
        }
        SecretSpans::Exact(merged(spans))
    }
}

/// `spans` in source order, a span inside another going with it, so none overlap.
fn merged(mut spans: Vec<SecretSpan>) -> Vec<SecretSpan> {
    spans.sort_unstable_by_key(|span| span.range.start);
    let mut out: Vec<SecretSpan> = Vec::with_capacity(spans.len());
    for mut span in spans {
        if let Some(last) = out.last() {
            if span.range.end <= last.range.end {
                continue;
            }
            span.range.start = span.range.start.max(last.range.end);
        }
        out.push(span);
    }
    out
}
