//! The longer spelling of a qualified pattern: a pattern matches a path as
//! written, so `Mutex::new($X)` does not match `std::sync::Mutex::new(x)`, and
//! a call on a bare name does not match a call that qualifies it
//! (`redact_lines($$$A)` against `crate::security::scan::redact_lines(x)`).
//! The same pattern with one capture and a separator before its leftmost
//! segment matches exactly those longer spellings.

use tree_sitter::Node;

use super::CompiledPattern;
use crate::structural::language::AgLanguage;
use crate::structural::octo::matching::node_text;
use crate::structural::octo::metavars::meta_from_node;

/// How a grammar qualifies a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Separator {
    /// A namespace path (`a::name`): a longer path before a literal segment
    /// is a qualification, never a receiver.
    Path,
    /// A member access (`a.name`): the qualifier can be a namespace, a module
    /// or a receiver, so only a call on a bare name has a longer spelling.
    Member,
}

impl Separator {
    fn text(self) -> &'static str {
        match self {
            Self::Path => "::",
            Self::Member => ".",
        }
    }
}

/// See [`CompiledPattern::qualifier`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedPattern {
    /// The leftmost segment the pattern spells (`Mutex`).
    pub head: String,
    /// The pattern with a capture before that segment
    /// (`$QUALIFIER::Mutex::new($X)`).
    pub broader: String,
}

/// A `::` path node and the field that holds its qualifier.
fn qualifier_field(kind: &str) -> Option<&'static str> {
    match kind {
        "scoped_identifier" | "scoped_type_identifier" => Some("path"),
        "qualified_identifier" => Some("scope"),
        _ => None,
    }
}

/// The kind of a bare callee: a plain name, not a capture.
const BARE_CALLEE_KIND: &str = "identifier";

/// The field that holds what a call calls.
const CALLEE: &str = "function";

/// The leftmost segments a longer path can be spelled before: a name, not a
/// capture or a path keyword (`crate`, `self`, `super`).
const HEAD_KINDS: [&str; 3] = ["identifier", "type_identifier", "namespace_identifier"];

/// The capture name the longer spelling binds.
const QUALIFIER: &str = "QUALIFIER";

impl CompiledPattern {
    /// The longer spelling of `pattern` (the text this pattern compiled
    /// from) in `lang`, which qualifies names with `separator`:
    ///
    /// - a `::` path (or a call on one) whose leftmost segment is a literal
    ///   name, in a grammar that qualifies with [`Separator::Path`], with a
    ///   fresh capture and `::` before that segment;
    /// - a call on a bare name, with a fresh capture and the grammar's
    ///   separator before the name, when the grammar reads that spelling as
    ///   the same call on a qualified callee.
    ///
    /// `None` for any other pattern.
    pub(in crate::structural::octo) fn qualifier(
        &self,
        lang: &AgLanguage,
        pattern: &str,
        separator: Separator,
    ) -> Option<QualifiedPattern> {
        if self.is_special() {
            return None;
        }
        let root = self.root()?;
        let path = root.child_by_field_name(CALLEE).unwrap_or(root);
        let mut head = path;
        while let Some(field) = qualifier_field(head.kind()) {
            head = head.child_by_field_name(field)?;
        }
        if head == path {
            return self.bare_callee(lang, pattern, root, path, separator);
        }
        if separator != Separator::Path
            || !HEAD_KINDS.contains(&head.kind())
            || meta_from_node(head, &self.source, self.expando).is_some()
        {
            return None;
        }
        self.broaden(pattern, root, head, separator)
    }

    /// The longer spelling of a call on the bare name `callee`, when the
    /// grammar reads it as the same call on a qualified callee: a name with
    /// a capture before it is a member or path, not another kind of call.
    fn bare_callee(
        &self,
        lang: &AgLanguage,
        pattern: &str,
        root: Node<'_>,
        callee: Node<'_>,
        separator: Separator,
    ) -> Option<QualifiedPattern> {
        if callee == root
            || callee.kind() != BARE_CALLEE_KIND
            || meta_from_node(callee, &self.source, self.expando).is_some()
        {
            return None;
        }
        let qualified = self.broaden(pattern, root, callee, separator)?;
        let longer = Self::new(lang, &qualified.broader).ok()?;
        let longer_root = longer.root()?;
        let longer_callee = longer_root.child_by_field_name(CALLEE)?;
        (!longer.is_special()
            && longer_root.kind() == root.kind()
            && longer_callee.kind() != callee.kind()
            && !longer_callee.has_error()
            && node_text(longer_callee, &longer.source).ends_with(&qualified.head))
        .then_some(qualified)
    }

    /// `pattern` with a fresh capture and `separator` before `head`.
    fn broaden(
        &self,
        pattern: &str,
        root: Node<'_>,
        head: Node<'_>,
        separator: Separator,
    ) -> Option<QualifiedPattern> {
        let at = self.pattern_offset(pattern, root, head)?;
        let capture = free_capture(pattern, QUALIFIER);
        Some(QualifiedPattern {
            head: node_text(head, &self.source).to_owned(),
            broader: format!(
                "{}${capture}{}{}",
                &pattern[..at],
                separator.text(),
                &pattern[at..]
            ),
        })
    }

    /// Where `node` (inside `root`) starts in the caller's `pattern`: the
    /// compiled source may wrap the pattern and respell its captures, so the
    /// root is found by its own text spelled back as the caller wrote it.
    fn pattern_offset(&self, pattern: &str, root: Node<'_>, node: Node<'_>) -> Option<usize> {
        let before = self.source.get(root.start_byte()..node.start_byte())?;
        let root_at = pattern.find(&self.expando.restore(node_text(root, &self.source)))?;
        Some(root_at + self.expando.restore(before).len())
    }
}

/// `name`, else the first `name<n>` the pattern does not spell.
pub(super) fn free_capture(pattern: &str, name: &str) -> String {
    std::iter::once(name.to_owned())
        .chain((2..).map(|n| format!("{name}{n}")))
        .find(|name| !pattern.contains(&format!("${name}")))
        .unwrap_or_default()
}
