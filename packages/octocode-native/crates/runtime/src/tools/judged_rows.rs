//! The judged source behind a row that names its file: read once a request,
//! through the read policy, and judged whole ([`crate::security::JudgedText`]).
//! Every tool that shows text of a local source cuts it from here by
//! position, so a search hit, an AST node, an LSP snippet and a read of the
//! same lines show the same redaction.
use crate::policy::path::PathPolicy;
use crate::security::{ContentSecurity, JudgedText};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The sources a row's files name, each read and judged once.
pub(crate) struct RowSources<'a> {
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    read: RefCell<HashMap<String, Option<Arc<JudgedText>>>>,
    /// Placeholders this request's cuts put in its rows.
    redacted: std::cell::Cell<usize>,
}

impl<'a> RowSources<'a> {
    pub(crate) fn new(paths: &'a PathPolicy, security: &'a ContentSecurity) -> Self {
        Self {
            paths,
            security,
            read: RefCell::default(),
            redacted: std::cell::Cell::new(0),
        }
    }

    /// The judged source of the file a row names as `shown`. `None` when the
    /// read policy refuses it, it cannot be read, or it is larger than a
    /// source judged whole: its rows are then left to the output gate.
    pub(crate) fn source(&self, shown: &str) -> Option<Arc<JudgedText>> {
        if let Some(known) = self.read.borrow().get(shown) {
            return known.clone();
        }
        let judged = self.paths.validate_read(shown).ok().and_then(|validated| {
            let limit = self.security.limits().whole_source;
            let bytes = crate::tools::source::read_bounded(&validated.canonical, limit).ok()?;
            let text = octocode_engine::text::decode(&bytes).text;
            Some(self.security.judge_source(&text, &validated.canonical))
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
    pub(crate) fn judge_leaf(
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
    pub(crate) fn judge_name(
        &self,
        leaf: &mut Value,
        source: &JudgedText,
        at: Option<(usize, usize)>,
    ) {
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
    pub(crate) fn put(&self, leaf: &mut Value, source: &JudgedText, shown: String, changed: bool) {
        if changed {
            let before = leaf.as_str().map_or(0, crate::security::placeholders);
            self.redacted.set(
                self.redacted.get() + crate::security::placeholders(&shown).saturating_sub(before),
            );
        }
        self.security.show(source.path(), &shown);
        if changed {
            *leaf = Value::String(shown);
        }
    }

    /// States on `data` that its text is not verbatim, when a cut of this
    /// request replaced any of it ([`crate::security::redacted_content_warning`]).
    pub(crate) fn warn(&self, data: &mut Value) {
        let redacted = self.redacted.take();
        if redacted > 0 {
            crate::response::paging::push_warnings(
                data,
                [crate::security::redacted_content_warning(redacted)],
            );
        }
    }
}

/// A 1-based JSON number as a line, or a 1-based column as a 0-based one.
pub(crate) fn number(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}
