//! Enclosing declarations of search hits, from the same declaration outline
//! behind astSearch `symbols` (and its content-keyed cache).
//!
//! A hit row names the innermost declaration around its line as
//! `enclosing: {symbolName, kind, line, endLine}` (`line` is the name line),
//! so a caller can cite or address the function a hit sits in without
//! reading it: `symbolName` + `line` anchor lspSearch, `line`–`endLine` is a
//! localFetch range. A hit on a declaration's own name line names the
//! declaration around that one. Consecutive hits in one declaration name it
//! once, on the first. A
//! declaration search (`fn foo`) names the owners of declaring hits only. A searched symbol whose declaration is among the
//! hits also yields a ready lspSearch references lead.

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

/// The part of the extractor's facts document an outline reads (the
/// engine's `GraphFactsDocument`, whose lines are `u32`).
#[derive(serde::Deserialize)]
struct Facts {
    declarations: Vec<FactDeclaration>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FactDeclaration {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    range: Option<Range>,
    #[serde(default)]
    selection_range: Option<Range>,
}

#[derive(serde::Deserialize)]
struct Range {
    #[serde(default)]
    start: Option<Position>,
    #[serde(default)]
    end: Option<Position>,
}

#[derive(serde::Deserialize)]
struct Position {
    #[serde(default)]
    line: Option<u32>,
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
        // Only the declarations' names, kinds and lines are read; every
        // other fact (and every other declaration field) is skipped
        // unparsed rather than built into a JSON tree.
        let facts: Facts = serde_json::from_str(&raw).ok()?;
        let line = |position: Option<&Position>| {
            position
                .and_then(|position| position.line)
                .map(|line| line.saturating_add(1))
        };
        let declarations = facts
            .declarations
            .into_iter()
            .filter_map(|item| {
                let range = item.range?;
                let start = line(range.start.as_ref())?;
                let end = line(range.end.as_ref())?.max(start);
                Some(Declaration {
                    kind: item.kind?,
                    name: item.name.filter(|name| !name.is_empty())?,
                    line: line(
                        item.selection_range
                            .as_ref()
                            .and_then(|range| range.start.as_ref()),
                    )
                    .unwrap_or(start),
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

    /// Declaration `owner`: its name, kind, name line and last line, so a
    /// read of it needs no outline first.
    pub(super) fn label(&self, owner: usize) -> Option<super::types::Enclosing> {
        let declaration = self.declarations.get(owner)?;
        Some(super::types::Enclosing {
            symbol_name: declaration.name.clone(),
            kind: declaration.kind.clone(),
            line: declaration.line,
            end_line: declaration.end.max(declaration.line),
        })
    }

    /// The innermost declaration spanning `line` other than one named on
    /// `line` itself.
    #[cfg(test)]
    pub(super) fn enclosing(&self, line: u32) -> Option<super::types::Enclosing> {
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
        let named = |line| {
            outline.enclosing(line).map(|enclosing| {
                (
                    enclosing.kind,
                    enclosing.symbol_name,
                    enclosing.line,
                    enclosing.end_line,
                )
            })
        };
        // X2: the enclosing declaration's full kind, name, name line and
        // last line (a single-line one ends where it starts).
        assert_eq!(
            named(5),
            Some(("method".into(), "try_read_output".into(), 3, 6))
        );
        assert_eq!(named(3), Some(("impl".into(), "Harness".into(), 2, 7)));
        assert_eq!(named(10), Some(("function".into(), "free".into(), 9, 11)));
        assert_eq!(outline.enclosing(8), None);
        assert!(outline.declares("try_read_output", 3));
        assert!(!outline.declares("try_read_output", 10));
    }

    /// One declaration: kind, name, name line, first and last line.
    type Row = (String, String, u32, u32, u32);

    /// The declarations an outline holds.
    fn rows(outline: &Outline) -> Vec<Row> {
        outline
            .declarations
            .iter()
            .map(|d| (d.kind.clone(), d.name.clone(), d.line, d.start, d.end))
            .collect()
    }

    /// The outline read through a whole JSON tree, as it was read before
    /// the typed facts: the reference the typed read must equal.
    fn tree_rows(raw: &str) -> Option<Vec<Row>> {
        let facts: serde_json::Value = serde_json::from_str(raw).ok()?;
        let line = |item: &serde_json::Value, pointer: &str| {
            item.pointer(pointer)
                .and_then(serde_json::Value::as_u64)
                .and_then(|line| u32::try_from(line).ok())
                .map(|line| line.saturating_add(1))
        };
        Some(
            facts["declarations"]
                .as_array()?
                .iter()
                .filter_map(|item| {
                    let start = line(item, "/range/start/line")?;
                    let end = line(item, "/range/end/line")?.max(start);
                    Some((
                        item["kind"].as_str()?.to_owned(),
                        item["name"]
                            .as_str()
                            .filter(|name| !name.is_empty())?
                            .to_owned(),
                        line(item, "/selectionRange/start/line").unwrap_or(start),
                        start,
                        end,
                    ))
                })
                .collect(),
        )
    }

    /// The typed read of the facts document keeps exactly the declarations
    /// the JSON-tree read kept, for the oxc (TS/JS) and tree-sitter paths.
    #[test]
    fn the_typed_outline_reads_what_the_json_tree_read() {
        let ts = "export class Box<T> {\n  private value: T;\n  constructor(v: T) { this.value = v; }\n  get(): T {\n    return this.value;\n  }\n}\nexport function make(): Box<number> {\n  const inner = () => 1;\n  return new Box(inner());\n}\ninterface Shape { area(): number }\nnamespace NS { export const k = 1; }\n";
        let py = "class A:\n    def m(self):\n        return 1\n\ndef f(x):\n    def g():\n        pass\n    return g\n";
        for (source, path) in [
            (SOURCE, "enclosing-test/a.rs"),
            (ts, "enclosing-test/b.ts"),
            (ts, "enclosing-test/b.tsx"),
            (py, "enclosing-test/c.py"),
        ] {
            let raw = octocode_engine::portable::extract_declarations(source, path)
                .expect("supported language");
            let typed = rows(&Outline::of(source, path).expect("outline"));
            assert!(!typed.is_empty(), "{path}");
            assert_eq!(Some(typed), tree_rows(&raw), "{path}");
        }
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
