//! Text a language server wrote, shown as the files it is about are judged.
//!
//! A hover, a diagnostic message, a symbol's name or detail has no place in
//! a file: the server computed it. Alone it reads as plain data, because the
//! key that names a value a secret is in the source, not in the server's
//! text. So every row leaving the resolution ladder passes [`judge`]: the
//! files the row is about (the anchor, every file the row names, and the
//! files an operation says declare what it describes, [`is_about`]) are
//! judged whole, once a request, and each value that judgment replaced is
//! replaced in the server's text ([`JudgedText::scrubbed`]), with the
//! placeholder a read of the file shows. The output gate judges what is left.
//!
//! A server also quotes values from files the row does not name (the literal
//! type of a constant declared elsewhere). No judgment covers those, so a
//! string literal in type text that no judged file holds is withheld
//! ([`withhold_unread`]): a read of a judged file would not show it, and its
//! own file was never asked whether a key names it a secret.
use super::LspFailure;
use super::render::uri_to_path;
use super::source::read_bounded_source_async;
use octocode_engine::security::REDACTION_PREFIX;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::security::cut::REDACTED;
use octocode_runtime_base::security::{ContentSecurity, JudgedText};
use serde_json::Value;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

tokio::task_local! {
    /// Files this page's server text is about beyond the ones its row names
    /// (see [`is_about`]).
    static ABOUT: RefCell<Vec<String>>;
}

/// Run one page, returning with its answer the files its operation said its
/// server text is about ([`is_about`]).
pub(super) async fn naming_files<F: Future>(page: F) -> (F::Output, Vec<String>) {
    ABOUT
        .scope(RefCell::default(), async {
            let answer = page.await;
            (answer, ABOUT.with(RefCell::take))
        })
        .await
}

/// Notes that the server text of this page is about the file at `path`
/// though no row field names it: the file that declares what a hover
/// describes. Outside [`naming_files`] (an operation run alone) nothing is
/// noted, and [`judge`] is not reached either.
pub(super) fn is_about(path: String) {
    let _ = ABOUT.try_with(|about| about.borrow_mut().push(path));
}

/// The files this page noted with [`is_about`] so far: what a kept answer
/// notes again when it is served ([`super::answer_memo`]).
pub(super) fn noted() -> Vec<String> {
    ABOUT
        .try_with(|about| about.borrow().clone())
        .unwrap_or_default()
}

/// [`is_about`] for every file a location answer names (`uri` or `path`).
pub(super) fn is_about_answer(answer: &Value) {
    let mut files = Vec::new();
    named_files(answer, None, &mut files);
    files.into_iter().for_each(is_about);
}

/// Row fields that hold calls and page facts the runtime wrote: nothing
/// under them is server text, and a call must stay executable.
const RUNTIME_BLOCKS: [&str; 3] = ["next", "related", "pagination"];

/// Row fields whose string the runtime wrote: a file's name, text cut from
/// the judged source by position ([`super::locations::judge_content`]), and
/// the row's own discriminators.
const RUNTIME_LEAVES: [&str; 6] = [
    "path",
    "uri",
    "workspaceRoot",
    "operation",
    "status",
    "errorCode",
];

/// The text of a hit row: the runtime's own, cut from the judged source by
/// position ([`super::locations::judge_content`]), where the row states the
/// line it was cut at ([`LOCATED_BY`]).
const HIT_TEXT: &str = "value";
/// The fields a located row states its place by: a public row its `line`,
/// a row as the server answered it its `uri` and `range`.
const LOCATED_BY: [&str; 3] = ["line", "uri", "range"];

/// What kind of text a leaf is, which decides the string literals in it
/// that need a judged file to hold them ([`withhold_unread`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wrote {
    /// Type text (a hover): code whose literals the server may have taken
    /// from any file.
    Types,
    /// A diagnostic message: prose that quotes names, and code inside those
    /// quotes.
    Message,
    /// Anything else (a symbol's name or detail, an error): text of the
    /// files the row names.
    Names,
}

/// Every string leaf of `value` a server may have written, with its kind.
fn server_leaves<'v>(
    value: &'v mut Value,
    key: Option<&str>,
    wrote: Wrote,
    out: &mut Vec<(&'v mut String, Wrote)>,
) {
    match value {
        Value::String(text) => {
            if key.is_some_and(|key| RUNTIME_LEAVES.contains(&key)) {
                return;
            }
            let wrote = match (key, wrote) {
                (Some("message"), Wrote::Names) => Wrote::Message,
                _ => wrote,
            };
            out.push((text, wrote));
        }
        Value::Array(items) => {
            for item in items {
                server_leaves(item, key, wrote, out);
            }
        }
        Value::Object(fields) => {
            // A located row's hit text is cut from the judged source by
            // position; a `value` anywhere else (a hover's marked text) is
            // the server's.
            let located = LOCATED_BY.iter().any(|place| fields.contains_key(*place));
            for (name, field) in fields {
                if RUNTIME_BLOCKS.contains(&name.as_str()) || (located && name == HIT_TEXT) {
                    continue;
                }
                let wrote = if name == "hover" { Wrote::Types } else { wrote };
                server_leaves(field, Some(name), wrote, out);
            }
        }
        _ => {}
    }
}

/// The files `value` names in a `path` or `uri` field, outside its calls.
fn named_files(value: &Value, key: Option<&str>, out: &mut Vec<String>) {
    match value {
        Value::String(text) if matches!(key, Some("path" | "uri")) && !text.is_empty() => {
            out.push(uri_to_path(text));
        }
        Value::Array(items) => items.iter().for_each(|item| named_files(item, key, out)),
        Value::Object(fields) => {
            for (name, field) in fields {
                if !RUNTIME_BLOCKS.contains(&name.as_str()) {
                    named_files(field, Some(name), out);
                }
            }
        }
        _ => {}
    }
}

/// What [`scrub`] changed in a row.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Scrubbed {
    /// Placeholders put where a judged file's redacted value stood.
    pub(super) replaced: usize,
    /// String literals withheld because no judged file holds them.
    pub(super) withheld: usize,
}

/// The one place server-written text is judged: every string leaf of `row`
/// a server may have written, with each value the whole-source pass
/// replaced in any of `judged` shown as that pass's placeholder, then with
/// the string literals no judged file holds withheld: always in type text
/// and inside a message's quotes, and in every other leaf when `unjudged`
/// (a file the row is about could not be judged whole).
pub(super) fn scrub(row: &mut Value, judged: &[Arc<JudgedText>], unjudged: bool) -> Scrubbed {
    let mut leaves = Vec::new();
    server_leaves(row, None, Wrote::Names, &mut leaves);
    let mut read = ReadTexts::of(judged);
    let mut done = Scrubbed::default();
    for (text, wrote) in leaves {
        let mut shown: Option<String> = None;
        for source in judged {
            if let Some(scrubbed) = source.scrubbed(shown.as_deref().unwrap_or(text)) {
                shown = Some(scrubbed);
            }
        }
        if let Some(shown) = &shown {
            done.replaced += octocode_runtime_base::security::placeholders(shown)
                .saturating_sub(octocode_runtime_base::security::placeholders(text));
        }
        let quoted = match wrote {
            Wrote::Types => Some(Quoted::Anywhere),
            Wrote::Message => Some(Quoted::InsideQuotes),
            Wrote::Names => unjudged.then_some(Quoted::Anywhere),
        };
        if let Some(quoted) = quoted
            && let Some((withheld, count)) =
                withhold_unread(shown.as_deref().unwrap_or(text), quoted, &mut read)
        {
            shown = Some(withheld);
            done.withheld += count;
        }
        if let Some(shown) = shown {
            *text = shown;
        }
    }
    done
}

/// Judges the server text of `row` as the file it quotes reads, not as the
/// markdown the server wrote: a server escapes punctuation (clangd writes
/// `secret\_Key`), and the output gate reads the escape as part of the key
/// and so judges the line differently from the same line in a file read.
/// A leaf whose gate verdict differs once its escapes are undone is shown
/// unescaped with that verdict applied. Returns whether any value was hidden.
pub(super) fn judge_unescaped(row: &mut Value, security: &ContentSecurity) -> bool {
    let mut leaves = Vec::new();
    server_leaves(row, None, Wrote::Names, &mut leaves);
    let mut hid = false;
    for (text, _) in leaves {
        let plain = markdown_unescaped(text);
        if plain == *text {
            continue;
        }
        let escaped = security.sanitize_leaf(text);
        let judged = security.sanitize_leaf(&plain);
        if escaped != judged {
            hid |= judged.is_some();
            *text = judged.unwrap_or(plain);
        }
    }
    hid
}

/// Which quoted spans of a text are string literals.
#[derive(Clone, Copy)]
enum Quoted {
    /// Every `"…"` and `'…'` span: the text is code.
    Anywhere,
    /// The `"…"` and `'…'` spans inside a quoted span (`'…'`, `"…"` or
    /// `` `…` ``): the text is prose that quotes names and types, and a
    /// string literal is a quote within such a quote (`'"value"'`,
    /// `"Literal['value']"`).
    InsideQuotes,
}

/// The text a read serves of each judged file, without its whitespace (a
/// server prints a declaration on its own line breaks), made when first asked.
struct ReadTexts<'a> {
    judged: &'a [Arc<JudgedText>],
    squeezed: Vec<Option<String>>,
}

impl<'a> ReadTexts<'a> {
    fn of(judged: &'a [Arc<JudgedText>]) -> Self {
        Self {
            judged,
            squeezed: vec![None; judged.len()],
        }
    }

    /// Whether a read of a judged file shows `literal`, as the server wrote
    /// it or with its markdown escapes undone: a server writing markdown
    /// escapes punctuation in the text it quotes (clangd writes the key
    /// `secret_Key` as `secret\_Key`), and a read shows the source unescaped.
    fn holds(&mut self, literal: &str) -> bool {
        let wanted = squeeze(literal);
        let unescaped = squeeze(&markdown_unescaped(literal));
        self.judged
            .iter()
            .zip(&mut self.squeezed)
            .any(|(source, squeezed)| {
                let read = squeezed.get_or_insert_with(|| {
                    squeeze(source.redaction().map_or(source.raw(), |(read, _)| read))
                });
                read.contains(&wanted) || read.contains(&unescaped)
            })
    }
}

/// `text` without its whitespace.
fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `text` with each markdown backslash escape undone: a `\` before ASCII
/// punctuation stands for that character (CommonMark).
fn markdown_unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek().copied() {
            Some(escaped) if c == '\\' && escaped.is_ascii_punctuation() => {
                out.push(escaped);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// `text` with the content of each string literal no judged file holds
/// replaced by [`REDACTED`], and how many; `None` when it holds none. A
/// literal that is a placeholder already stays.
fn withhold_unread(
    text: &str,
    quoted: Quoted,
    read: &mut ReadTexts<'_>,
) -> Option<(String, usize)> {
    let literals = match quoted {
        Quoted::Anywhere => quoted_spans(text, false),
        Quoted::InsideQuotes => quoted_spans(text, true)
            .into_iter()
            .flat_map(|outer| {
                let inner = text.get(outer.clone()).unwrap_or_default();
                quoted_spans(inner, false)
                    .into_iter()
                    .map(move |span| outer.start + span.start..outer.start + span.end)
            })
            .collect(),
    };
    let mut out = String::with_capacity(text.len());
    let mut kept = 0;
    let mut withheld = 0;
    for literal in literals {
        let Some(content) = text.get(literal.clone()) else {
            continue;
        };
        if content.trim().is_empty() || content.contains(REDACTION_PREFIX) || read.holds(content) {
            continue;
        }
        out.push_str(text.get(kept..literal.start).unwrap_or_default());
        out.push_str(REDACTED);
        kept = literal.end;
        withheld += 1;
    }
    (withheld > 0).then(|| {
        out.push_str(text.get(kept..).unwrap_or_default());
        (out, withheld)
    })
}

/// The contents of the quoted spans of `text`, in order and never nested:
/// `"…"`, `'…'`, and `` `…` `` when `ticks`. A span stays on one line and a
/// backslash escapes the next character. A `'` opens only where no word
/// character is before it and closes only where none is after it, so an
/// apostrophe and a lifetime (`&'a str`) are not quotes.
fn quoted_spans(text: &str, ticks: bool) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let word = |at: Option<&u8>| {
        at.is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte >= 0x80)
    };
    let closing = |open: usize| -> Option<usize> {
        let quote = bytes[open];
        let mut at = open + 1;
        while let Some(byte) = bytes.get(at) {
            match *byte {
                b'\n' => return None,
                b'\\' => at += 1,
                byte if byte == quote => {
                    return (quote != b'\'' || !word(bytes.get(at + 1))).then_some(at);
                }
                _ => {}
            }
            at += 1;
        }
        None
    };
    let mut spans = Vec::new();
    let mut at = 0;
    while let Some(byte) = bytes.get(at) {
        let opens = match *byte {
            b'"' => true,
            b'\'' => !word(at.checked_sub(1).and_then(|before| bytes.get(before))),
            b'`' => ticks,
            _ => false,
        };
        match opens.then(|| closing(at)).flatten() {
            Some(end) => {
                spans.push(at + 1..end);
                at = end + 1;
            }
            None => at += 1,
        }
    }
    spans
}

/// Judge the server-written text of `row`, the answer to a request anchored
/// at `anchor` (its file, or the workspace root of a request without one):
/// [`scrub`] it with the whole-source judgment of each file it is about —
/// the anchor, every file the row names, and `about`, the files its
/// operation named ([`is_about`]). Each file is read once here, through the
/// read policy and the bounded source read, and judged once a request
/// ([`octocode_runtime_base::security::JudgedFiles`]); the request's cancellation and
/// deadline are checked between files. A file the policy refuses, that
/// cannot be read, or that is longer than a source judged whole leaves the
/// row unjudged for it: its string literals are withheld. The row states
/// what was replaced and what was withheld.
pub(super) async fn judge(
    row: &mut Value,
    anchor: &str,
    about: Vec<String>,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<Judged, LspFailure> {
    let mut files = vec![anchor.to_owned()];
    files.extend(about);
    named_files(row, None, &mut files);
    let request = super::scope::judged_files();
    let mut seen = std::collections::HashSet::new();
    let mut judged = Vec::new();
    let mut unjudged = 0usize;
    for file in files {
        let Ok(readable) = paths.validate_read(&file) else {
            // A directory (a request without a file) holds no text. Anything
            // else the read policy refuses is a file left unjudged.
            let directory = paths
                .validate(&file)
                .is_ok_and(|valid| valid.canonical.is_dir());
            if !directory && seen.insert(std::path::PathBuf::from(&file)) {
                unjudged += 1;
            }
            continue;
        };
        if !seen.insert(readable.canonical.clone()) {
            continue;
        }
        cancel.check().map_err(LspFailure::cancelled)?;
        match read_bounded_source_async(readable.canonical.clone()).await {
            Ok(read) if read.text.len() <= security.limits().whole_source => {
                let text = read.text;
                let path = &readable.canonical;
                judged.push(request.judge(&text, path, |digest| {
                    octocode_tools_shared::judged_rows::judged(&text, digest, path, security)
                }));
            }
            _ => unjudged += 1,
        }
    }
    let done = scrub(row, &judged, unjudged > 0);
    let unescaped = judge_unescaped(row, security);
    if done.replaced > 0 || done.withheld > 0 || unescaped {
        octocode_runtime_base::security::mark_redacted(row);
    }
    Ok(Judged(()))
}

/// Proof that a row's server text was judged ([`judge`]): the forms a row is
/// served in that pack its fields into other strings or groups
/// ([`super::render::served_forms`]) are built only from a judged row, so no
/// reorder can pack a value the judgment would have replaced.
pub(super) struct Judged(());

/// A [`Judged`] for tests of the served forms alone.
#[cfg(test)]
pub(super) fn judged_for_tests() -> Judged {
    Judged(())
}

#[cfg(test)]
mod tests;
