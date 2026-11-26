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
        outline.enclosing(line, 0).map(|enclosing| {
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
    assert_eq!(outline.enclosing(8, 0), None);
    assert!(outline.declares("try_read_output", 3));
    assert!(!outline.declares("try_read_output", 10));
}

/// A hit declares the declaration whose name its matched bytes cover or
/// reach into, wherever the match starts: the name alone, part of it, the
/// head up to it, the whole line. A match that ends before the name, a
/// local binding, a use and a line outside every name declare nothing.
#[test]
fn a_hit_covering_a_name_declares_it() {
    let outline = Outline::of(SOURCE, "enclosing-test/a.rs").expect("rust outline");
    let declared = |line: u32, needle: &str| {
        let text = SOURCE.lines().nth(line as usize - 1).expect("line");
        let byte = u32::try_from(text.find(needle).expect("needle")).expect("u32");
        let len = u32::try_from(needle.len()).expect("u32");
        outline
            .declared(line, byte, len)
            .and_then(|index| outline.label(index))
            .map(|d| (d.symbol_name, d.line, d.end_line))
    };
    let method = Some(("try_read_output".to_owned(), 3, 6));
    // Every match that reaches the name names the same declaration: the
    // rule reads the span, not the pattern that made it.
    for needle in [
        "try_read_output",
        "try_read",
        "read_output",
        "fn try_read_output",
        "fn try_read_output(&self) {",
        "fn try",
    ] {
        assert_eq!(declared(3, needle), method, "{needle}");
    }
    for needle in ["fn", "fn ", "self", "(&self)"] {
        assert_eq!(declared(3, needle), None, "{needle}");
    }
    assert_eq!(
        declared(1, "struct Harness;"),
        Some(("Harness".to_owned(), 1, 1))
    );
    assert_eq!(declared(4, "x"), None);
    assert_eq!(declared(4, "let x = 1;"), None);
    assert_eq!(declared(10, "try_read_output"), None);
    // A hit of no known length is the byte it starts at.
    let name = u32::try_from(
        SOURCE
            .lines()
            .nth(2)
            .expect("line")
            .find("try")
            .expect("name"),
    )
    .expect("u32");
    assert!(outline.declared(3, name, 0).is_some());
    assert!(outline.declared(3, 0, 0).is_none());
}

#[test]
fn rust_macro_arm_hits_belong_to_the_named_macro() {
    for attribute in ["macro_export", "allow(dead_code)"] {
        let source = format!(
            "macro_rules! wrapper {{\n\
             ($item:item) => {{\n\
             /// Example: option.unwrap()\n\
             #[{attribute}]\n\
             $item\n\
             }};\n\
             }}\n"
        );
        let outline = Outline::of(&source, "enclosing-test/macro-arm.rs").expect("outline");
        for line in 2..=6 {
            let owner = outline.enclosing(line, 0).expect("containing macro");
            assert_eq!(owner.symbol_name, "wrapper", "{attribute}, line {line}");
            assert_eq!((owner.line, owner.end_line), (1, 7));
        }
    }
}

/// On a one-line (minified) file every declaration starts on line 1: a hit
/// names the declaration its byte sits in, a hit on a header up to its name
/// names the one around it, and a hit past a declaration's end is outside it.
#[test]
fn hits_on_one_line_name_the_declaration_their_byte_sits_in() {
    let source =
        "function alpha(){return first}class Beta{gamma(){return second}}const tail=third;\n";
    let outline = Outline::of(source, "enclosing-test/min.js").expect("js outline");
    let byte = |needle: &str| u32::try_from(source.find(needle).expect("needle")).expect("u32");
    let owner = |needle| {
        outline
            .enclosing(1, byte(needle))
            .map(|enclosing| enclosing.symbol_name)
    };
    assert_eq!(owner("first").as_deref(), Some("alpha"));
    assert_eq!(owner("second").as_deref(), Some("gamma"));
    assert_eq!(owner("gamma").as_deref(), Some("Beta"));
    assert_eq!(owner("third").as_deref(), Some("tail"));
    assert_eq!(owner("alpha"), None);
    assert_eq!(owner("class"), None);
}

/// A `#define` owns its line terminator; its range still ends on its
/// own line, so a hit on the next macro is not attributed to it.
#[test]
fn consecutive_c_macros_end_on_their_own_line() {
    let source = "#define OBJ_ENCODING_STREAM 10\n#define OBJ_ENCODING_LISTPACK 11\nint use(void) {\n    return OBJ_ENCODING_LISTPACK;\n}\n";
    let outline = Outline::of(source, "enclosing-test/object.h").expect("c outline");
    assert_eq!(outline.enclosing(2, 0), None);
    assert!(outline.declares("OBJ_ENCODING_LISTPACK", 2));
    assert_eq!(
        outline
            .named("OBJ_ENCODING_STREAM", 1)
            .and_then(|index| outline.label(index))
            .map(|d| (d.kind, d.line, d.end_line)),
        Some(("macro".into(), 1, 1))
    );
    assert_eq!(
        outline
            .enclosing(4, 0)
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
        .map(|d| (d.kind.clone(), d.name.clone(), d.line, d.from.0, d.end))
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

/// Independent requests read the same typed declarations: the one kept
/// document of the text.
#[test]
fn an_outline_agrees_with_independent_declaration_requests() {
    let path = "enclosing-test/shared-parse.rs";
    let source = format!("{SOURCE}\n// shared parse\n");
    assert!(Outline::of(&source, path).is_some());
    let first = octocode_tools_shared::declarations::file_document(&source, path).expect("parse");
    let again = octocode_tools_shared::declarations::file_document(&source, path).expect("parse");
    assert_eq!(first, again);
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert!(Outline::of("not a supported file", "enclosing-test/a.unknownext").is_none());
}

#[test]
fn symbols_are_bare_identifiers_or_named_by_a_declaration_head() {
    assert_eq!(searched_symbol("toValidURL"), Some("toValidURL"));
    assert_eq!(
        searched_symbol("fn merge_near_windows"),
        Some("merge_near_windows")
    );
    assert_eq!(searched_symbol("$scope"), Some("$scope"));
    assert!(declaration_search("def handler"));
    // Modifiers before the keyword name the same declaration.
    for (head, name) in [
        ("pub fn push_warnings", "push_warnings"),
        ("pub(crate) fn walk_builder", "walk_builder"),
        ("export default class Cache", "Cache"),
        ("async def handler", "handler"),
    ] {
        assert_eq!(searched_symbol(head), Some(name), "{head}");
        assert!(declaration_search(head), "{head}");
    }
    // A head written up to its signature's opening names the identifier
    // the declared word starts with.
    for (head, name) in [
        ("pub fn walk_builder(", "walk_builder"),
        ("fn apply<T>(", "apply"),
        ("def handler(self", "handler"),
    ] {
        assert_eq!(searched_symbol(head), Some(name), "{head}");
        assert!(declaration_search(head), "{head}");
    }
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
        "pub use walk",
        "pub fn",
        "9lives",
        "",
    ] {
        assert_eq!(searched_symbol(text), None, "{text}");
    }
}

/// An identifier continues with exactly the engine's identifier characters
/// (a combining mark included).
#[test]
fn searched_symbols_continue_with_engine_identifier_characters() {
    for name in ["cafe\u{301}", "x$y", "a_b9", "naïve", "a-b", "a.b", "a²"] {
        let continues = name
            .chars()
            .skip(1)
            .all(octocode_engine::text::is_ident_char);
        assert_eq!(searched_symbol(name).is_some(), continues, "{name:?}");
    }
}

/// A file over the enclosing cap is not outlined (its hits keep no
/// `enclosing`); one at the cap still is.
#[test]
fn files_over_the_enclosing_cap_are_not_outlined() {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let path = std::path::Path::new("big.rs");
    let line = "fn f() {}\n";
    let at_cap = line.repeat(ENCLOSING_MAX_BYTES / line.len());
    assert!(outline_of(at_cap.as_bytes(), path, &security).is_some());
    let over = format!("{at_cap}{line}{line}");
    assert!(over.len() > ENCLOSING_MAX_BYTES);
    assert!(outline_of(over.as_bytes(), path, &security).is_none());
}

/// A hit inside a method names the method and lists the declarations
/// holding it before it, outermost first, so its class is named without
/// another read; each is listed once and the hit indexes its innermost.
#[test]
fn a_hits_declaration_follows_the_declarations_holding_it() {
    let outline = Outline::of(SOURCE, "enclosing-test/a.rs").expect("rust outline");
    let chain = |line| outline.chain(outline.owner(line, 0).expect("owner"));
    let names = |chain: &[super::super::types::Enclosing]| {
        chain
            .iter()
            .map(|d| d.symbol_name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&chain(5)), ["Harness", "try_read_output"]);
    assert_eq!(names(&chain(10)), ["free"]);
    let mut declarations = Vec::new();
    let method = super::owners_of(&mut declarations, vec![Some(chain(5))]);
    let header = super::owners_of(&mut declarations, vec![Some(chain(3))]);
    assert_eq!(names(&declarations), ["Harness", "try_read_output"]);
    assert_eq!(method, Some(super::super::types::Owners::One(1)));
    assert_eq!(header, Some(super::super::types::Owners::One(0)));
    let python = Outline::of(
        "class A:\n    def m(self):\n        return 1\n",
        "enclosing-test/a.py",
    )
    .expect("python outline");
    let held = python.chain(python.owner(3, 0).expect("owner"));
    assert_eq!(names(&held), ["A", "m"]);
}
