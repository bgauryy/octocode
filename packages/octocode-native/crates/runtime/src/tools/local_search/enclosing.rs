//! Enclosing declarations of search hits, from the same declaration outline
//! behind astSearch `symbols` (and its content-keyed cache).
//!
//! A hit row names the innermost declaration around its line as
//! `enclosing: {symbolName, kind, line, endLine}` (`line` is the name line),
//! so a caller can cite or address the function a hit sits in without
//! reading it: `symbolName` + `line` anchor lspSearch, `line`–`endLine` is a
//! localFetch range. A hit on a declaration's own name line names the
//! declaration around that one. Every hit row inside a declaration names
//! it, so no row reads as having none. A declaration search (`fn foo`)
//! names the owners of declaring hits only. A searched symbol whose
//! declaration is among the hits also yields a ready lspSearch references
//! lead.

use super::handoff::*;
use super::types::*;
use crate::security::ContentSecurity;
use sha2::{Digest, Sha256};

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
    /// It reads the typed declarations parse every reader of the file
    /// shares ([`crate::tools::declarations_cache::file_document`]): one
    /// parse per source version, never a JSON round trip.
    pub(super) fn of(source: &str, canonical_path: &str) -> Option<Self> {
        let document = crate::tools::declarations_cache::file_document(source, canonical_path)?;
        let declarations = document
            .declarations
            .iter()
            .filter(|item| !item.name.is_empty())
            .map(|item| {
                let start = item.range.start.line.saturating_add(1);
                Declaration {
                    kind: item.kind.clone(),
                    name: item.name.clone(),
                    line: item.selection_range.start.line.saturating_add(1),
                    start,
                    end: item.range.end.line.saturating_add(1).max(start),
                }
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
        Some((keyword, rest)) if octocode_engine::search::is_declaration_keyword(keyword) => {
            rest.trim()
        }
        Some(_) => return None,
        None => text,
    };
    let mut chars = name.chars();
    let first = chars.next()?;
    ((first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    .then_some(name)
}

/// Serialized chars an `enclosing` field adds besides its value.
pub(super) const ENCLOSING_FIELD_CHARS: usize = ",\"enclosing\":".len();

/// Serialized chars an `enclosingLine` field adds besides its number.
pub(super) const ENCLOSING_LINE_FIELD_CHARS: usize = ",\"enclosingLine\":".len();

/// What a page cut reserves per hit row of a file it has not outlined (a
/// later page's, or one it could not read; see
/// [`super::layout::NamePricing`]) for its `enclosing` name: the field, the
/// object's keys, a kind, two line numbers and a typical symbol name. A
/// longer name still goes on its row; the page runs that much over budget.
pub(super) const ENCLOSING_RESERVE_CHARS: usize = ENCLOSING_FIELD_CHARS + 80;

/// The current outline of a shown file, or `None` when it is too large,
/// unreadable, unsupported, or no longer the bytes a stored scan matched.
pub(super) fn shown_outline(
    source: &std::path::Path,
    expected: Option<Option<super::sources::Digest>>,
    security: &ContentSecurity,
) -> Option<super::enclosing::Outline> {
    let bytes = shown_bytes(source, expected)?;
    outline_of(&bytes, source, security)
}

/// The current bytes of a shown file within the parse cap, or `None` when
/// it is larger, unreadable, no longer a regular file (a FIFO or link
/// swapped in since the walk), or no longer the bytes a stored scan matched.
pub(super) fn shown_bytes(
    source: &std::path::Path,
    expected: Option<Option<super::sources::Digest>>,
) -> Option<Vec<u8>> {
    let limit = octocode_engine::signatures::MAX_PARSE_SIZE;
    let bytes = super::sources::read_entry(source, limit)?;
    if let Some(expected) = expected
        && expected != Some(<[u8; 32]>::from(Sha256::digest(&bytes)))
    {
        return None;
    }
    Some(bytes)
}

/// Set `matchKeys` on each shown JSON or TOML file with hit rows: the key
/// path of every hit line, read from the file's current bytes, in page
/// order while the page stays within `page_budget`. Returns the note for
/// files whose keys did not fit, never dropped silently.
pub(super) fn annotate_match_keys(
    files: &mut [SearchFile],
    output_root: &std::path::Path,
    page_budget: usize,
    expected: &dyn Fn(&std::path::Path) -> Option<Option<super::sources::Digest>>,
    security: &ContentSecurity,
) -> Option<String> {
    let mut used = octocode_engine::json_size::bytes(&*files).unwrap_or(usize::MAX);
    let mut over_budget = 0usize;
    for file in files.iter_mut() {
        let source = output_root.join(&file.path);
        let Some(rows) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            continue;
        };
        if !crate::tools::match_keys::is_keyed_path(&source) {
            continue;
        }
        let mut lines = rows
            .iter()
            .flat_map(row_lines)
            .filter(|line| *line > 0)
            .map(|line| line as usize)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines.dedup();
        let limit = octocode_engine::signatures::MAX_PARSE_SIZE;
        let Some(bytes) = shown_bytes(&source, expected(&source)) else {
            continue;
        };
        let Ok(text) = security.decode_source_bytes(&bytes, limit) else {
            continue;
        };
        let keys = crate::tools::match_keys::line_key_paths(&source, &text, &lines);
        if keys.is_empty() {
            continue;
        }
        let added = octocode_engine::json_size::bytes(&crate::tools::match_keys::MatchKeys(&keys))
            .unwrap_or(usize::MAX)
            .saturating_add(MATCH_KEYS_FIELD_CHARS);
        if over_budget == 0 && used.saturating_add(added) <= page_budget {
            used += added;
            file.match_keys = keys;
        } else {
            over_budget += 1;
        }
    }
    (over_budget > 0).then(|| {
        let noun = if over_budget == 1 { "file" } else { "files" };
        format!(
            "Key paths (matchKeys) omitted on {over_budget} later {noun} to keep the page within its size budget."
        )
    })
}

/// Serialized chars around a file's key map.
const MATCH_KEYS_FIELD_CHARS: usize = ",\"matchKeys\":".len();

/// Largest file outlined for `enclosing` names. Parsing is linear (a 4.5 MB
/// generated Rust file took 1 s of a 1.2 s search, 2026-10-08) and files this
/// large are almost always generated; their hits keep no enclosing name.
pub(super) const ENCLOSING_MAX_BYTES: usize = 1024 * 1024;

/// The outline of a shown file's `bytes`, read within the parse cap; none
/// for a file over [`ENCLOSING_MAX_BYTES`].
pub(super) fn outline_of(
    bytes: &[u8],
    source: &std::path::Path,
    security: &ContentSecurity,
) -> Option<super::enclosing::Outline> {
    if bytes.len() > ENCLOSING_MAX_BYTES {
        return None;
    }
    let limit = octocode_engine::signatures::MAX_PARSE_SIZE;
    let text = security.decode_source_bytes(bytes, limit).ok()?;
    super::enclosing::Outline::of(&text, &source.to_string_lossy())
}

/// Lines a shown row stands for: every matched line of a merged block.
pub(super) fn row_lines(row: &SearchMatch) -> Vec<u32> {
    row.match_lines.clone().unwrap_or_else(|| vec![row.line])
}

/// Name the enclosing declaration of every hit row of a page's files
/// (`enclosing` on the first row inside a declaration, `enclosingLine` on
/// the later ones) and find the first shown hit that declares `symbol`.
/// Each file is parsed once. The page cut charged the names' size, so a
/// name is never traded for budget; rows of a file over 1 MiB are counted
/// in the returned note, never dropped silently.
pub(super) fn annotate_enclosing(
    files: &mut [SearchFile],
    symbol: Option<&str>,
    declaring: Option<&str>,
    output_root: &std::path::Path,
    expected: &dyn Fn(&std::path::Path) -> Option<Option<super::sources::Digest>>,
    mut outlines: super::verify::Outlines,
    security: &ContentSecurity,
) -> (Option<Definition>, Option<String>) {
    let mut names: Vec<Vec<Option<super::types::Enclosing>>> = Vec::with_capacity(files.len());
    let mut definition = None;
    let mut oversized_rows = 0usize;
    for file in files.iter() {
        let Some(matches) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            names.push(Vec::new());
            continue;
        };
        let source = output_root.join(&file.path);
        // The secret check already read and outlined most shown files.
        let outline = match outlines.remove(&source) {
            Some(outline) => outline,
            None => shown_outline(&source, expected(&source), security),
        };
        let Some(outline) = outline else {
            // Only a file whose language has declarations lost them to its
            // size (a 3 MiB `uv.lock` never had any).
            if outlined_language(&source)
                && std::fs::metadata(&source)
                    .is_ok_and(|meta| meta.len() > ENCLOSING_MAX_BYTES as u64)
            {
                oversized_rows += matches.len();
            }
            names.push(Vec::new());
            continue;
        };
        if definition.is_none()
            && let Some(symbol) = symbol
            && let Some((line, (kind, start, end))) = matches
                .iter()
                .flat_map(row_lines)
                .find_map(|line| Some((line, outline.declaration(symbol, line)?)))
        {
            definition = Some(Definition {
                source: source.to_string_lossy().into_owned(),
                line,
                kind: kind.to_owned(),
                start,
                end,
            });
        }
        names.push(row_owners(&outline, matches, declaring));
    }
    for (file, names) in files.iter_mut().zip(names) {
        let mut named = std::collections::HashMap::new();
        for (row, name) in file.matches.iter_mut().flatten().zip(names) {
            let Some(name) = name else { continue };
            // A name already given by the nearest earlier row at its line
            // is a reference; a different declaration on that line is named
            // in full, and becomes what the line refers to.
            if named.get(&name.line) == Some(&name) {
                row.enclosing_line = Some(name.line);
            } else {
                named.insert(name.line, name.clone());
                row.enclosing = Some(name);
            }
        }
    }
    let note = (oversized_rows > 0).then(|| {
        format!(
            "Enclosing declarations (enclosing) are not named in files over 1 MiB; {} there {} none.",
            hit_rows(oversized_rows),
            have(oversized_rows)
        )
    });
    (definition, note)
}

/// `1 hit row`, `3 hit rows`.
fn hit_rows(rows: usize) -> String {
    format!("{rows} hit {}", if rows == 1 { "row" } else { "rows" })
}

pub(super) fn have(rows: usize) -> &'static str {
    if rows == 1 { "has" } else { "have" }
}

/// Whether `source`'s language has a declaration outline (by extension).
pub(super) fn outlined_language(source: &std::path::Path) -> bool {
    source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            octocode_engine::signatures::languages::find_entry(&extension.to_ascii_lowercase())
                .is_some()
        })
}

/// Each row's enclosing declaration, on every row inside one: so no row
/// reads as having none. With `declaring` (a declaration search), only rows that declare
/// that symbol are named.
pub(super) fn row_owners(
    outline: &super::enclosing::Outline,
    rows: &[SearchMatch],
    declaring: Option<&str>,
) -> Vec<Option<super::types::Enclosing>> {
    rows.iter()
        .map(|row| {
            let named = declaring.is_none_or(|symbol| {
                row_lines(row)
                    .iter()
                    .any(|line| outline.declares(symbol, *line))
            });
            named
                .then(|| outline.owner(row_lines(row)[0]))
                .flatten()
                .and_then(|owner| outline.label(owner))
        })
        .collect()
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
        // The enclosing declaration's full kind, name, name line and
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

    /// A `#define` owns its line terminator; its range still ends on its
    /// own line, so a hit on the next macro is not attributed to it.
    #[test]
    fn consecutive_c_macros_end_on_their_own_line() {
        let source = "#define OBJ_ENCODING_STREAM 10\n#define OBJ_ENCODING_LISTPACK 11\nint use(void) {\n    return OBJ_ENCODING_LISTPACK;\n}\n";
        let outline = Outline::of(source, "enclosing-test/object.h").expect("c outline");
        assert_eq!(outline.enclosing(2), None);
        assert!(outline.declares("OBJ_ENCODING_LISTPACK", 2));
        assert_eq!(
            outline.declaration("OBJ_ENCODING_STREAM", 1),
            Some(("macro", 1, 1))
        );
        assert_eq!(
            outline
                .enclosing(4)
                .map(|e| (e.symbol_name, e.line, e.end_line)),
            Some(("use".into(), 3, 5))
        );
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

    /// The outline keeps every named declaration with its 1-based name,
    /// first and last line, for the oxc (TS/JS) and tree-sitter paths.
    #[test]
    fn the_outline_keeps_every_declaration_with_its_lines() {
        let ts = "export class Box<T> {\n  private value: T;\n  constructor(v: T) { this.value = v; }\n  get(): T {\n    return this.value;\n  }\n}\nexport function make(): Box<number> {\n  const inner = () => 1;\n  return new Box(inner());\n}\ninterface Shape { area(): number }\nnamespace NS { export const k = 1; }\n";
        let py = "class A:\n    def m(self):\n        return 1\n\ndef f(x):\n    def g():\n        pass\n    return g\n";
        let row = |kind: &str, name: &str, line, start, end| -> Row {
            (kind.into(), name.into(), line, start, end)
        };
        let ts_rows = [
            row("class", "Box", 1, 1, 7),
            row("property", "value", 2, 2, 2),
            row("constructor", "constructor", 3, 3, 3),
            row("method", "get", 4, 4, 6),
            row("function", "make", 8, 8, 11),
            row("function", "inner", 9, 9, 9),
            row("interface", "Shape", 12, 12, 12),
            row("method", "area", 12, 12, 12),
            row("namespace", "NS", 13, 13, 13),
            row("constant", "k", 13, 13, 13),
        ];
        for (source, path, expected) in [
            (
                SOURCE,
                "enclosing-test/a.rs",
                vec![
                    row("struct", "Harness", 1, 1, 1),
                    row("impl", "Harness", 2, 2, 7),
                    row("method", "try_read_output", 3, 3, 6),
                    row("function", "free", 9, 9, 11),
                ],
            ),
            (ts, "enclosing-test/b.ts", ts_rows.to_vec()),
            (ts, "enclosing-test/b.tsx", ts_rows.to_vec()),
            (
                py,
                "enclosing-test/c.py",
                vec![
                    row("class", "A", 1, 1, 3),
                    row("method", "m", 2, 2, 3),
                    row("function", "f", 5, 5, 8),
                    row("function", "g", 6, 6, 7),
                ],
            ),
        ] {
            let outline = rows(&Outline::of(source, path).expect("outline"));
            assert_eq!(outline, expected, "{path}");
        }
    }

    /// An outline reads the shared typed parse: the parse it built is the
    /// one the next reader of the same source version takes, unparsed again.
    #[test]
    fn an_outline_reads_the_shared_declarations_parse() {
        let path = "enclosing-test/shared-parse.rs";
        let source = format!("{SOURCE}\n// shared parse\n");
        assert!(Outline::of(&source, path).is_some());
        let first = crate::tools::declarations_cache::file_document(&source, path).expect("parse");
        let again = crate::tools::declarations_cache::file_document(&source, path).expect("parse");
        assert!(std::sync::Arc::ptr_eq(&first, &again));
        assert!(Outline::of("not a supported file", "enclosing-test/a.unknownext").is_none());
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
        // Every keyword the relevance ranking reads as a declaration counts.
        for text in ["mod walk", "object Cache", "val limit", "macro_rules! row"] {
            assert!(declaration_search(text), "{text}");
        }
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

#[cfg(test)]
mod enclosing_cap_tests {
    /// A file over the enclosing cap is not outlined (its hits keep no
    /// `enclosing`); one at the cap still is.
    #[test]
    fn files_over_the_enclosing_cap_are_not_outlined() {
        let security = crate::security::ContentSecurity;
        let path = std::path::Path::new("big.rs");
        let line = "fn f() {}\n";
        let at_cap = line.repeat(super::ENCLOSING_MAX_BYTES / line.len());
        assert!(super::outline_of(at_cap.as_bytes(), path, &security).is_some());
        let over = format!("{at_cap}{line}{line}");
        assert!(over.len() > super::ENCLOSING_MAX_BYTES);
        assert!(super::outline_of(over.as_bytes(), path, &security).is_none());
    }
}
