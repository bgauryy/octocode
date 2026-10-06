//! Enclosing declarations of search hits, from the same declaration outline
//! behind astSearch `symbols` (and its content-keyed cache).
//!
//! A hit row names the innermost declaration around its line as
//! `in: "<kind> name@<line>"` (`fn`, `const`, `var`, `prop` shorten the
//! commonest kinds), `<line>` being the declaration's name line, so a caller
//! can cite or address the function a hit sits in without reading it. A hit
//! on a declaration's own name line names the declaration around that one.
//! Every name carries the declaration's last line, `name@<line>-<end>`.
//! Consecutive hits in one declaration name it once, on the first. A
//! declaration search (`fn foo`) names the owners of declaring hits only. A searched symbol whose declaration is among the
//! hits also yields a ready lspSearch references lead.

use serde_json::Value;

/// One declaration of a file's outline, 1-based lines.
struct Declaration {
    kind: String,
    name: String,
    /// The name line.
    line: u32,
    start: u32,
    end: u32,
}

pub(super) struct Outline {
    declarations: Vec<Declaration>,
}

impl Outline {
    /// The outline of `source`; `None` when no extractor supports the file.
    pub(super) fn of(source: &str, canonical_path: &str) -> Option<Self> {
        let raw = crate::tools::ast_search::declarations_cache::extract(
            source,
            canonical_path,
            false,
            || octocode_engine::portable::extract_declarations(source, canonical_path),
        )?;
        let facts: Value = serde_json::from_str(&raw).ok()?;
        let items = facts["declarations"].as_array()?;
        let line = |item: &Value, pointer: &str| {
            item.pointer(pointer)
                .and_then(Value::as_u64)
                .and_then(|line| u32::try_from(line).ok())
                .map(|line| line.saturating_add(1))
        };
        let declarations = items
            .iter()
            .filter_map(|item| {
                let start = line(item, "/range/start/line")?;
                let end = line(item, "/range/end/line")?.max(start);
                Some(Declaration {
                    kind: item["kind"].as_str()?.to_owned(),
                    name: item["name"]
                        .as_str()
                        .filter(|name| !name.is_empty())?
                        .to_owned(),
                    line: line(item, "/selectionRange/start/line").unwrap_or(start),
                    start,
                    end,
                })
            })
            .collect();
        Some(Self { declarations })
    }

    /// The innermost declaration spanning `line` other than one named on
    /// `line` itself (its index in the outline).
    pub(super) fn owner(&self, line: u32) -> Option<usize> {
        self.declarations
            .iter()
            .enumerate()
            .filter(|(_, d)| d.start <= line && line <= d.end && d.line != line)
            .min_by_key(|(_, d)| (d.end - d.start, std::cmp::Reverse(d.start)))
            .map(|(index, _)| index)
    }

    /// Declaration `owner` as `kind name@line-end` (`kind name@line` for
    /// a one-line declaration): its name line and last line, so a read of
    /// it needs no outline first.
    pub(super) fn label(&self, owner: usize) -> Option<String> {
        let declaration = self.declarations.get(owner)?;
        let kind = match declaration.kind.as_str() {
            "function" => "fn",
            "constant" => "const",
            "variable" => "var",
            "property" => "prop",
            kind => kind,
        };
        Some(if declaration.end > declaration.line {
            format!(
                "{kind} {}@{}-{}",
                declaration.name, declaration.line, declaration.end
            )
        } else {
            format!("{kind} {}@{}", declaration.name, declaration.line)
        })
    }

    /// The innermost declaration spanning `line` other than one named on
    /// `line` itself, as `kind name@line-end`.
    #[cfg(test)]
    pub(super) fn enclosing(&self, line: u32) -> Option<String> {
        self.label(self.owner(line)?)
    }

    /// Whether `line` is the name line of a declaration named `name`.
    pub(super) fn declares(&self, name: &str, line: u32) -> bool {
        self.declaration(name, line).is_some()
    }

    /// The declaration named `name` on name line `line`: its kind and its
    /// first and last line.
    pub(super) fn declaration(&self, name: &str, line: u32) -> Option<(&str, u32, u32)> {
        self.declarations
            .iter()
            .find(|d| d.line == line && d.name == name)
            .map(|d| (d.kind.as_str(), d.start, d.end))
    }
}

/// Keywords that may precede a searched declaration name (`fn foo`,
/// `def foo`, `class Foo`).
const DECLARATION_KEYWORDS: &[&str] = &[
    "fn",
    "def",
    "func",
    "function",
    "class",
    "struct",
    "enum",
    "trait",
    "interface",
    "type",
    "const",
    "let",
    "var",
    "impl",
    "module",
    "namespace",
];

/// Whether the search names a declaration (`fn foo`, `class Foo`) rather
/// than a bare identifier.
pub(super) fn declaration_search(search_text: &str) -> bool {
    searched_symbol(search_text).is_some() && search_text.trim().contains(char::is_whitespace)
}

/// The identifier a search names: a bare identifier, or one after a
/// declaration keyword.
pub(super) fn searched_symbol(search_text: &str) -> Option<&str> {
    let text = search_text.trim();
    let name = match text.split_once(char::is_whitespace) {
        Some((keyword, rest)) if DECLARATION_KEYWORDS.contains(&keyword) => rest.trim(),
        Some(_) => return None,
        None => text,
    };
    let mut chars = name.chars();
    let first = chars.next()?;
    ((first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    .then_some(name)
}

#[cfg(test)]
mod tests {
    use super::{Outline, declaration_search, searched_symbol};

    const SOURCE: &str = "struct Harness;\n\
impl Harness {\n\
    fn try_read_output(&self) {\n\
        let x = 1;\n\
        x.unwrap();\n\
    }\n\
}\n\
\n\
fn free() {\n\
    try_read_output();\n\
}\n";

    #[test]
    fn hits_name_their_innermost_declaration_and_definitions_the_one_around() {
        let outline = Outline::of(SOURCE, "enclosing-test/a.rs").expect("rust outline");
        assert_eq!(
            outline.enclosing(5).as_deref(),
            Some("method try_read_output@3-6")
        );
        assert_eq!(outline.enclosing(3).as_deref(), Some("impl Harness@2-7"));
        assert_eq!(outline.enclosing(10).as_deref(), Some("fn free@9-11"));
        assert_eq!(outline.enclosing(8), None);
        assert!(outline.declares("try_read_output", 3));
        assert!(!outline.declares("try_read_output", 10));
    }

    #[test]
    fn symbols_are_bare_identifiers_or_follow_a_declaration_keyword() {
        assert_eq!(searched_symbol("toValidURL"), Some("toValidURL"));
        assert_eq!(
            searched_symbol("fn merge_near_windows"),
            Some("merge_near_windows")
        );
        assert_eq!(searched_symbol("$scope"), Some("$scope"));
        assert!(declaration_search("def handler"));
        assert!(!declaration_search("toValidURL"));
        assert!(!declaration_search("foo bar"));
        for text in [
            "ctx.Done()",
            ".unwrap()",
            "a|b",
            "let x = 1",
            "foo bar",
            "9lives",
            "",
        ] {
            assert_eq!(searched_symbol(text), None, "{text}");
        }
    }
}
