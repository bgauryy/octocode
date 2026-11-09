//! Literal tokens of a pattern that look like metavariables: a metavariable is
//! a whole node whose text is the sigil and an upper-case name
//! ([`meta_from_node`]), so `search_$X` (one identifier) and `$x` (a lower-case
//! name) stay literal text that no source spells. Read from the compiled
//! tree the matcher reads, so the fact cannot disagree with it.

use tree_sitter::Node;

use super::CompiledPattern;
use super::qualified::{free_captures, is_name_char as is_word_char};
use crate::structural::octo::matching::node_text;
use crate::structural::octo::metavars::{is_capture_name, meta_from_node};

/// A literal token of a compiled pattern that spells a metavariable sigil.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralSigil {
    /// The token as the caller wrote it (`search_$X`, `$x`).
    pub token: String,
    pub form: SigilForm,
    /// The kind of the leaf that holds the token, when it is a named one: a
    /// source node of that kind spelling the token is the same name.
    pub kind: Option<String>,
    /// A glued token that is a whole named leaf of the pattern's node, reached
    /// from it by fields that each name one child: the rule, applied to the
    /// matched node, that holds exactly the nodes the token meant there (the
    /// leaf's kind in the same place, its text the token with each capture
    /// spelling one or more characters).
    place: Option<serde_json::Value>,
}

/// Why a [`LiteralSigil`] is no metavariable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigilForm {
    /// The sigil is part of a longer token or node (`search_$X`, `"$X and"`):
    /// a metavariable is a whole node.
    Glued,
    /// A sigil and a name with lower-case letters (`$x`, `$$$args`): a
    /// capture name is upper-case letters, digits or `_`.
    LowerCase,
}

/// The pattern, and the rules its matched node must also hold, that match
/// exactly what a pattern's literal sigils meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigilRepair {
    /// What the caller wrote (the pattern, or the context), rewritten.
    pub pattern: String,
    /// One rule per glued token, in the place of the capture that replaced
    /// it: `{"has":{"field":"function","kind":"identifier","regex":"^search_.+$"}}`.
    pub constraints: Vec<serde_json::Value>,
}

/// The field of `parent` that names `node`, when it names no other child.
fn sole_field(parent: Node<'_>, node: Node<'_>) -> Option<String> {
    let mut cursor = parent.walk();
    let mut fields = Vec::new();
    let mut field = None;
    if cursor.goto_first_child() {
        loop {
            fields.push(cursor.field_name());
            if cursor.node() == node {
                field = cursor.field_name();
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    let field = field?;
    (fields.iter().filter(|named| **named == Some(field)).count() == 1).then(|| field.to_owned())
}

/// The regex a glued `token` (as written) holds of the leaf it is: its
/// literal text, each `$NAME` one or more characters, anchored at both ends.
/// `None` when a sigil run is not one `$` and a capture name, or a name ending
/// in `_` runs into more text (`$X_impl`: `X_` or `X` then `_impl`).
fn glued_regex(token: &str) -> Option<String> {
    let mut regex = String::from("^");
    let mut rest = token;
    while let Some((literal, after)) = rest.split_once('$') {
        regex.push_str(&regex::escape(literal));
        let (name, tail) = split_capture_name(after);
        if !is_capture_name(name) || (name.ends_with('_') && !tail.is_empty()) {
            return None;
        }
        regex.push_str(".+");
        rest = tail;
    }
    regex.push_str(&regex::escape(rest));
    regex.push('$');
    Some(regex)
}

/// `text` split after its leading capture-name characters (upper-case
/// letters, digits, `_`): the name a `$` before it spells, and the rest.
fn split_capture_name(text: &str) -> (&str, &str) {
    text.char_indices()
        .find(|(_, ch)| !(*ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit()))
        .and_then(|(at, _)| text.split_at_checked(at))
        .unwrap_or((text, ""))
}

/// `text` with each whole `token` (no identifier character or sigil before
/// it, no identifier character after it) spelled `new`.
fn respelled(text: &str, token: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    for (at, _) in text.match_indices(token) {
        let before = text.get(..at).and_then(|head| head.chars().next_back());
        let after = text
            .get(at + token.len()..)
            .and_then(|tail| tail.chars().next());
        if at < rest
            || before.is_some_and(|ch| is_word_char(ch) || ch == '$')
            || after.is_some_and(is_word_char)
        {
            continue;
        }
        out.push_str(text.get(rest..at).unwrap_or_default());
        out.push_str(new);
        rest = at + token.len();
    }
    out.push_str(text.get(rest..).unwrap_or_default());
    out
}

impl CompiledPattern {
    fn is_sigil(&self, ch: char) -> bool {
        ch == '$' || self.expando.matches_leading(ch)
    }

    /// The tokens of `text` (runs of identifier characters and sigils) in
    /// which a sigil precedes an identifier character, with their offsets.
    fn sigil_tokens<'t>(&self, text: &'t str) -> Vec<(usize, &'t str)> {
        let mut tokens = Vec::new();
        let mut rest = text.char_indices().peekable();
        while let Some((from, ch)) = rest.next() {
            if !(is_word_char(ch) || self.is_sigil(ch)) {
                continue;
            }
            let mut end = from + ch.len_utf8();
            let mut spells = false;
            let mut previous = ch;
            while let Some(&(at, next)) = rest.peek() {
                if !(is_word_char(next) || self.is_sigil(next)) {
                    break;
                }
                spells |= self.is_sigil(previous) && !self.is_sigil(next);
                previous = next;
                end = at + next.len_utf8();
                rest.next();
            }
            if spells && let Some(token) = text.get(from..end) {
                tokens.push((from, token));
            }
        }
        tokens
    }

    /// Why the compiled `token` is no metavariable.
    fn sigil_form(&self, token: &str) -> SigilForm {
        let sigils = token.chars().take_while(|ch| self.is_sigil(*ch)).count();
        let name = token.trim_start_matches(|ch| self.is_sigil(ch));
        let lower_case = matches!(sigils, 1 | 3)
            && !name.chars().any(|ch| self.is_sigil(ch))
            && !is_capture_name(name)
            && is_capture_name(&name.to_ascii_uppercase());
        if lower_case {
            SigilForm::LowerCase
        } else {
            SigilForm::Glued
        }
    }

    /// Each leaf of the pattern's node (of a contextual pattern, its whole
    /// context) whose text holds a sigil before a name but which is no
    /// metavariable, as the caller wrote it in `pattern` (or the context);
    /// once per token, in source order. A sigil the caller did not write (an
    /// expando letter in a grammar's own text) is none.
    pub(in crate::structural::octo) fn literal_sigils(&self, pattern: &str) -> Vec<LiteralSigil> {
        let Some(root) = self.root() else {
            return Vec::new();
        };
        let read = match (&self.tree, self.selected_node) {
            (Some(tree), Some(_)) => tree.root_node(),
            _ => root,
        };
        let mut leaves = Vec::new();
        let mut pending = vec![read];
        while let Some(node) = pending.pop() {
            if meta_from_node(node, &self.source, self.expando).is_some() {
                continue;
            }
            if node.child_count() == 0 {
                leaves.push(node);
            } else {
                pending.extend(node.children(&mut node.walk()));
            }
        }
        leaves.sort_by_key(Node::start_byte);
        let mut found: Vec<LiteralSigil> = Vec::new();
        for leaf in leaves {
            let text = node_text(leaf, &self.source);
            for (at, token) in self.sigil_tokens(text) {
                let written = self.expando.restore(token);
                if !pattern.contains(&written) || found.iter().any(|seen| seen.token == written) {
                    continue;
                }
                let form = self.sigil_form(token);
                let place = (form == SigilForm::Glued && at == 0 && token.len() == text.len())
                    .then(|| place_of(root, leaf, &written))
                    .flatten();
                found.push(LiteralSigil {
                    form,
                    kind: leaf.is_named().then(|| leaf.kind().to_owned()),
                    place,
                    token: written,
                });
            }
        }
        found
    }

    /// `pattern`, whose literal sigils are `sigils`, rewritten to match what
    /// they meant, and the rules its matched node must then hold: each
    /// lower-case name upper-cased, and each glued token replaced by a fresh
    /// capture whose place ([`LiteralSigil::place`]) holds a node of the
    /// token's kind and spelling. `None` when a glued token has no place. The
    /// caller checks the rewrite compiles and spells no literal sigil.
    pub(in crate::structural::octo) fn sigil_rewrite(
        pattern: &str,
        sigils: &[LiteralSigil],
    ) -> Option<(String, Vec<serde_json::Value>)> {
        let mut repaired = pattern.to_owned();
        let mut constraints = Vec::new();
        for sigil in sigils.iter().filter(|sigil| sigil.form == SigilForm::Glued) {
            let place = sigil.place.clone()?;
            let (_, first) = sigil.token.split_once('$')?;
            let (base, _) = split_capture_name(first);
            let name = free_captures(&respelled(&repaired, &sigil.token, ""), base, 1).pop()?;
            repaired = respelled(&repaired, &sigil.token, &format!("${name}"));
            constraints.push(place);
        }
        for sigil in sigils
            .iter()
            .filter(|sigil| sigil.form == SigilForm::LowerCase)
        {
            repaired = respelled(&repaired, &sigil.token, &sigil.token.to_ascii_uppercase());
        }
        Some((repaired, constraints))
    }
}

/// The rule [`LiteralSigil::place`] states for the glued `token` that is the
/// whole `leaf` under `root`; `None` for a leaf outside `root` (a contextual
/// pattern's context), an anonymous one, or one a field does not name alone
/// at some step.
fn place_of(root: Node<'_>, leaf: Node<'_>, token: &str) -> Option<serde_json::Value> {
    if !leaf.is_named() {
        return None;
    }
    let mut rule = serde_json::json!({"kind": leaf.kind(), "regex": glued_regex(token)?});
    let mut node = leaf;
    while node != root {
        let parent = node.parent()?;
        // The field first: it names where the rest of the step applies.
        let mut step = serde_json::Map::new();
        step.insert("field".into(), serde_json::json!(sole_field(parent, node)?));
        if let serde_json::Value::Object(fields) = rule {
            step.extend(fields);
        }
        rule = serde_json::json!({"has": step});
        if parent != root {
            rule["kind"] = serde_json::json!(parent.kind());
        }
        node = parent;
    }
    Some(rule)
}

#[cfg(test)]
mod tests {
    use super::{glued_regex, respelled, split_capture_name};

    /// Non-ASCII text beside a sigil splits on characters, never inside one.
    #[test]
    fn non_ascii_text_beside_a_sigil_splits_on_characters() {
        assert_eq!(split_capture_name("Xü"), ("X", "ü"));
        assert_eq!(split_capture_name("XY"), ("XY", ""));
        assert_eq!(glued_regex("é_$Xü").as_deref(), Some("^é_.+ü$"));
        assert_eq!(respelled("é$x.f($x)ü", "$x", "$X"), "é$x.f($X)ü");
    }

    #[test]
    fn a_respelling_replaces_only_whole_tokens() {
        assert_eq!(respelled("$x.f($xy, $x)", "$x", "$X"), "$X.f($xy, $X)");
        assert_eq!(respelled("a$x $$x", "$x", "$X"), "a$x $$x");
    }
}
