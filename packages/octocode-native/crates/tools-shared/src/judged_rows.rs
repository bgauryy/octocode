//! The judged source behind a row that names its file: read once a request,
//! through the read policy, and judged whole ([`octocode_runtime_base::security::JudgedText`]).
//! Every tool that shows text of a local source cuts it from here by
//! position, so a search hit, an AST node, an LSP snippet and a read of the
//! same lines show the same redaction. What the pass made of a text is kept
//! by the text's digest ([`judged`]), so a text is scanned once while its
//! pass is kept, whichever row, page, request or tool asks next.
use crate::page_memo::PageMemo;
use octocode_engine::security::Edits;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::security::{ContentSecurity, JudgedText, WholePass};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// What the whole-source pass made of a text it changed
/// ([`WholePass`]), as the store keeps it; `None` for a
/// pass that replaced nothing.
type Pass = Option<Kept>;

#[derive(Clone)]
struct Kept {
    /// The text a read serves.
    clean: Arc<str>,
    /// What the pass replaced, by position.
    edits: Arc<Edits>,
    /// `clean` is the scanner's own text ([`WholePass::scanned`]).
    scanned: bool,
}

/// The pass of every source a tool judged whole, kept by the digest of the
/// text judged, the path it was judged at and the scanner's limits: all a
/// pass reads. A source's text is judged once while its pass is kept, across
/// rows, pages, requests and tools; a file whose text changed has another
/// digest and is judged again. A kept pass holds no source text, and a pass
/// that replaced nothing holds nothing. Weighed against the one process
/// budget, in memory only ([`PageMemo`]): eviction or expiry costs a scan.
static PASSES: PageMemo<Pass> = PageMemo::new();

/// The whole-source pass over `raw`, the decoded text of the file at `path`
/// whose bytes hash to `digest`: the one kept of this text at this path
/// ([`PASSES`]), else a scan now ([`JudgedText::pass`]), whose pass is then
/// kept.
fn pass(raw: &str, digest: &[u8; 32], path: &Path, security: &ContentSecurity) -> Pass {
    let key = format!("{}\n{}", hex::encode(digest), path.to_string_lossy());
    let policy = format!("{:?}", security.limits());
    if let Some(pass) = PASSES.get(&key, &policy, Clone::clone) {
        return pass;
    }
    let kept = |pass: WholePass| Kept {
        clean: Arc::from(pass.clean),
        edits: Arc::new(pass.edits),
        scanned: pass.scanned,
    };
    let pass: Pass = JudgedText::pass(raw, path, security).map(kept);
    let weight = pass
        .as_ref()
        .map_or(0, |kept| kept.clean.len() + kept.edits.heap_bytes());
    PASSES.put(key, policy, weight, pass.clone());
    pass
}

/// `raw`, the decoded text of the file at `path` whose bytes hash to
/// `digest`, judged whole ([`JudgedText`]): by the pass kept of this text at
/// this path ([`PASSES`]), else by a scan now, whose pass is then kept.
pub fn judged(raw: &str, digest: &[u8; 32], path: &Path, security: &ContentSecurity) -> JudgedText {
    let pass = pass(raw, digest, path, security);
    JudgedText::judged_as(
        raw,
        path,
        pass.as_ref().map(|kept| (&*kept.clean, &*kept.edits)),
    )
}

/// What `security` makes of `raw`, the decoded text of the file at `path`
/// with no private-key block in it, scanned whole
/// ([`ContentSecurity::sanitize_text`]): the text a read of the source cuts
/// its windows from, `raw` itself when the scan left it as it is. It is the
/// text of the one pass every tool's rows of this source are cut from
/// ([`pass`]), so a second read of an unchanged file scans nothing. A pass
/// that did not say what it replaced kept no text of the scanner's: the
/// text is then scanned for this read.
pub(crate) fn scanned_whole<'a>(
    raw: &'a str,
    path: &Path,
    security: &ContentSecurity,
) -> std::borrow::Cow<'a, str> {
    let digest = octocode_engine::digest::sha256_digest(raw.as_bytes());
    match pass(raw, &digest, path, security) {
        None => raw.into(),
        Some(kept) if kept.scanned => kept.clean.to_string().into(),
        Some(_) => security.sanitize_text(raw, Some(path)).content,
    }
}

/// What a search's whole read of a local source found, kept by the digest
/// of the file's bytes, the path it was judged at and the scanner's limits,
/// as [`PASSES`] keeps a text's pass: a later search of the same bytes
/// proves them by their digest and reads no line of them for a scan. It
/// holds no text of the source.
static WHOLE_SOURCES: PageMemo<WholeSource> = PageMemo::new();

/// A local source a search read whole ([`WHOLE_SOURCES`]).
#[derive(Clone, Copy)]
pub struct WholeSource {
    /// How many lines the read found.
    pub lines: usize,
    /// The digest of the text the read judged (its lines, as the search
    /// decoded them), when the pass replaced any of it: the key its pass is
    /// kept by ([`judged`]). `None` when the pass replaced nothing, so its
    /// rows show their own bytes.
    pub redacted_text: Option<[u8; 32]>,
}

/// What a whole read of the source at `path` whose bytes hash to `bytes`
/// found ([`keep_whole_source`]), when one is kept.
pub fn whole_source(
    bytes: &[u8; 32],
    path: &Path,
    security: &ContentSecurity,
) -> Option<WholeSource> {
    let key = format!("{}\n{}", hex::encode(bytes), path.to_string_lossy());
    WHOLE_SOURCES.get(&key, &format!("{:?}", security.limits()), |found| *found)
}

/// Keep what a whole read of the source at `path` whose bytes hash to
/// `bytes` found.
pub fn keep_whole_source(
    bytes: &[u8; 32],
    path: &Path,
    security: &ContentSecurity,
    found: WholeSource,
) {
    let key = format!("{}\n{}", hex::encode(bytes), path.to_string_lossy());
    let policy = format!("{:?}", security.limits());
    WHOLE_SOURCES.put(key, policy, std::mem::size_of::<WholeSource>(), found);
}

/// [`judged`] of a text whose digest is not known yet.
pub fn judged_text(raw: &str, path: &Path, security: &ContentSecurity) -> JudgedText {
    let digest = octocode_engine::digest::sha256_digest(raw.as_bytes());
    judged(raw, &digest, path, security)
}

/// The sources a row's files name, each read and judged once.
pub struct RowSources<'a> {
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    read: RefCell<HashMap<String, Option<Arc<JudgedText>>>>,
    /// Placeholders this request's cuts put in its rows.
    redacted: std::cell::Cell<usize>,
    /// The rows' positions count the text a read shows of each source, and
    /// their texts are cut from it ([`JudgedText::as_shown`]).
    shown: bool,
}

impl<'a> RowSources<'a> {
    pub fn new(paths: &'a PathPolicy, security: &'a ContentSecurity) -> Self {
        Self {
            paths,
            security,
            read: RefCell::default(),
            redacted: std::cell::Cell::new(0),
            shown: false,
        }
    }

    /// [`Self::new`] for rows whose positions and texts count the text a
    /// read shows of their source (astSearch matches that text): each source
    /// is judged as that text, its placeholders shown as themselves.
    pub fn as_shown(paths: &'a PathPolicy, security: &'a ContentSecurity) -> Self {
        Self {
            shown: true,
            ..Self::new(paths, security)
        }
    }

    /// The judged source of the file a row names as `shown`. `None` when the
    /// read policy refuses it, it cannot be read, or it is larger than a
    /// source judged whole: its rows are then left to the output gate.
    pub fn source(&self, shown: &str) -> Option<Arc<JudgedText>> {
        if let Some(known) = self.read.borrow().get(shown) {
            return known.clone();
        }
        let judged = self.paths.validate_read(shown).ok().and_then(|validated| {
            let limit = self.security.limits().whole_source;
            let bytes = crate::source::read_bounded(&validated.canonical, limit).ok()?;
            let text = octocode_engine::text::decode(&bytes).text;
            let path = &validated.canonical;
            let judged = self.security.judge_source(&text, path, |digest| {
                judged(&text, digest, path, self.security)
            });
            Some(if self.shown {
                Arc::new(judged.as_shown())
            } else {
                judged
            })
        });
        self.read
            .borrow_mut()
            .insert(shown.to_owned(), judged.clone());
        judged
    }

    /// Replace the string at `leaf` by what `source` shows of it, a fragment
    /// that starts on the 1-based `line` at the 0-based `column` (see
    /// [`JudgedText::fragment`]), and record it as shown. `suffix` is text
    /// the runtime wrote after the fragment (a clip marker).
    pub fn judge_leaf(
        &self,
        leaf: &mut Value,
        source: &JudgedText,
        (line, column): (usize, Option<usize>),
        suffix: &str,
    ) {
        let Some(text) = leaf.as_str() else {
            return;
        };
        let body = text.strip_suffix(suffix).unwrap_or(text);
        let (shown, judged) = source.fragment(line, column, body);
        if !judged {
            return;
        }
        let changed = shown != body;
        let shown = if suffix.is_empty() || body.len() == text.len() {
            shown.into_owned()
        } else {
            format!("{shown}{suffix}")
        };
        self.put(leaf, source, shown, changed);
    }

    /// Replace the name at `leaf` (a declaration's, a symbol's) by what
    /// `source` shows of it. A name is an identifier: at `at`, its 1-based
    /// line and 0-based column, it is the source's own bytes there, hidden
    /// only where the pass replaced bytes of the name itself
    /// ([`JudgedText::cut`]), whatever the rest of its line holds. A name
    /// that is not the source's text at that place, or states none (a
    /// display name, a name another row declares), is a fact with no place:
    /// each value the pass replaced shows in it as its placeholder
    /// ([`JudgedText::scrubbed`]), and a name that holds none is left to the
    /// output gate.
    pub fn judge_name(&self, leaf: &mut Value, source: &JudgedText, at: Option<(usize, usize)>) {
        let Some(text) = leaf.as_str() else {
            return;
        };
        let placed = at.and_then(|(line, column)| source.place(line, Some(column), text));
        let shown = match placed {
            Some(at) => source.cut(at).into_owned(),
            None => match source.scrubbed(text) {
                Some(scrubbed) => scrubbed,
                None => return,
            },
        };
        let changed = shown != text;
        self.put(leaf, source, shown, changed);
    }

    /// Put `shown`, what `source` shows of the text at `leaf`, there, and
    /// record it as shown.
    pub fn put(&self, leaf: &mut Value, source: &JudgedText, shown: String, changed: bool) {
        if changed {
            let before = leaf
                .as_str()
                .map_or(0, octocode_runtime_base::security::placeholders);
            self.redacted.set(
                self.redacted.get()
                    + octocode_runtime_base::security::placeholders(&shown).saturating_sub(before),
            );
        } else if self.shown && source.is_redacted() {
            // The shown text's placeholders stand in the leaf already.
            self.redacted
                .set(self.redacted.get() + octocode_runtime_base::security::placeholders(&shown));
        }
        self.security.show(source.path(), &shown);
        if changed {
            *leaf = Value::String(shown);
        }
    }

    /// Marks `data` redacted, when a cut of this request replaced any of its
    /// text ([`octocode_runtime_base::security::mark_redacted`]).
    pub fn warn(&self, data: &mut Value) {
        if self.redacted.take() > 0 {
            octocode_runtime_base::security::mark_redacted(data);
        }
    }
}

/// A 1-based JSON number as a line, or a 1-based column as a 0-based one.
pub fn number(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}

#[cfg(test)]
mod tests;
