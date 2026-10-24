use super::{ENCLOSING_MAX_BYTES, Outline, declaration_search, outline_of, searched_symbol};

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

/// A file over the enclosing cap is not outlined (its hits keep no
/// `enclosing`); one at the cap still is.
#[test]
fn files_over_the_enclosing_cap_are_not_outlined() {
    let security = crate::security::ContentSecurity;
    let path = std::path::Path::new("big.rs");
    let line = "fn f() {}\n";
    let at_cap = line.repeat(ENCLOSING_MAX_BYTES / line.len());
    assert!(outline_of(at_cap.as_bytes(), path, &security).is_some());
    let over = format!("{at_cap}{line}{line}");
    assert!(over.len() > ENCLOSING_MAX_BYTES);
    assert!(outline_of(over.as_bytes(), path, &security).is_none());
}
