use super::*;

fn resolve_char(file_name: &str, source: &str, symbol_name: &str, line_hint: u32) -> u32 {
    resolve_position_in_file_content(
        file_name,
        source,
        &LspFuzzyPosition {
            symbol_name: symbol_name.to_owned(),
            line_hint: Some(line_hint),
            order_hint: None,
        },
    )
    .expect("must resolve")
    .position
    .character
}

#[test]
fn resolves_utf16_character_for_non_ascii_prefix() {
    // The prefix contains both a BMP character and a surrogate pair.
    let prefix = "const label = 'é🌍'; const ";
    assert_eq!(
        resolve_char("demo.ts", &format!("{prefix}target = 1;\n"), "target", 1),
        prefix.encode_utf16().count() as u32
    );
}

#[test]
fn unicode_identifier_substrings_are_not_symbol_anchors() {
    for identifier in [
        "étarget",
        "targeté",
        "target\u{301}",
        "℘target",
        "target\u{200c}tail",
        "東京target",
        "𐐀target",
    ] {
        let source = format!("const {identifier} = 1;\n");
        for file_name in ["demo.ts", "demo.unknown"] {
            assert!(
                resolve_position_in_file_content(
                    file_name,
                    &source,
                    &LspFuzzyPosition {
                        symbol_name: "target".to_owned(),
                        line_hint: Some(1),
                        order_hint: None,
                    }
                )
                .is_err(),
                "must not resolve target inside {identifier} in {file_name}"
            );
        }
    }
}

#[test]
fn complete_unicode_identifiers_keep_utf16_positions() {
    let prefix = "const label = 'é🌍'; const ";
    for identifier in [
        "étarget",
        "targeté",
        "target\u{301}",
        "℘target",
        "target\u{200c}tail",
        "東京target",
        "𐐀target",
        "$target",
    ] {
        let source = format!("{prefix}{identifier} = 1;\n");
        for file_name in ["demo.ts", "demo.unknown"] {
            assert_eq!(
                resolve_char(file_name, &source, identifier, 1),
                prefix.encode_utf16().count() as u32,
                "{identifier} in {file_name}"
            );
        }
    }
}

fn resolve(file_name: &str, source: &str, symbol_name: &str, line_hint: u32) -> u32 {
    let result = resolve_position_in_file_content(
        file_name,
        source,
        &LspFuzzyPosition {
            symbol_name: symbol_name.to_owned(),
            line_hint: Some(line_hint),
            order_hint: None,
        },
    );
    match result {
        Ok(hit) => hit.found_at_line,
        Err(err) => panic!("failed to resolve {symbol_name} in {file_name}: {err}"),
    }
}

fn resolve_with_order(source: &str, symbol_name: &str, line_hint: u32, order: u32) -> u32 {
    resolve_position_in_file_content(
        "demo.ts",
        source,
        &LspFuzzyPosition {
            symbol_name: symbol_name.to_owned(),
            line_hint: Some(line_hint),
            order_hint: Some(order),
        },
    )
    .unwrap_or_else(|err| panic!("failed to resolve {symbol_name} order {order}: {err}"))
    .position
    .character
}

#[test]
fn order_hint_skips_duplicate_string_nodes_and_prefers_identifiers() {
    // `"foo"` yields a `string` node and a `string_fragment` child that both
    // point at column 11; they must collapse into one candidate, and the
    // real identifiers must be ranked before the string-literal mention.
    let source = "const x = \"foo\"; foo(); foo();\n";
    assert_eq!(resolve_with_order(source, "foo", 1, 0), 17);
    assert_eq!(resolve_with_order(source, "foo", 1, 1), 24);
    // The string-literal mention is not indexable while the line has
    // identifiers: an `occurrence` past them is an error naming their count.
    let literal = resolve_position_in_file_content(
        "demo.ts",
        source,
        &LspFuzzyPosition {
            symbol_name: "foo".to_owned(),
            line_hint: Some(1),
            order_hint: Some(2),
        },
    );
    assert!(
        literal
            .as_ref()
            .is_err_and(|err| err.to_string().contains("at most 1")),
        "{literal:?}"
    );
    // A line spelling the name only inside a literal still anchors there.
    assert_eq!(resolve_with_order("const y = \"foo\";\n", "foo", 1, 0), 11);
}

#[test]
fn tree_sitter_anchor_ignores_comment_on_requested_line() {
    let source = "/* target is mentioned in a block comment */\nfunction target() {}\n";
    assert_eq!(resolve("demo.ts", source, "target", 1), 2);
}

#[test]
fn tree_sitter_resolves_requested_language_matrix() {
    let cases = [
        ("demo.ts", "export function target() {}\n", 1),
        ("demo.tsx", "export const target = () => <div />;\n", 1),
        ("demo.js", "export function target() {}\n", 1),
        ("demo.jsx", "export const target = () => <div />;\n", 1),
        ("demo.py", "def target():\n    return 1\n", 1),
        ("demo.go", "package main\nfunc target() {}\n", 2),
        ("demo.rs", "fn target() {}\n", 1),
        ("demo.java", "class Target { void target() {} }\n", 1),
        ("demo.c", "void target() {}\n", 1),
        ("demo.cpp", "void target() {}\n", 1),
        (
            "demo.cu",
            "__global__ void target() {}\nvoid launch() { target<<<1, 1>>>(); }\n",
            1,
        ),
        ("demo.asm", "target:\n  mov %rax, %rbx\n", 1),
        ("demo.cs", "class Target { void target() {} }\n", 1),
        ("demo.sh", "target() { echo ok; }\n", 1),
        ("demo.json", "{\"target\": true}\n", 1),
        ("demo.yaml", "target: true\n", 1),
        ("demo.toml", "target = true\n", 1),
        ("demo.html", "<div id=\"target\"></div>\n", 1),
        ("demo.css", ".target { color: red; }\n", 1),
        ("demo.scss", ".target { color: red; }\n", 1),
        ("demo.less", ".target { color: red; }\n", 1),
    ];

    for (file_name, source, expected_line) in cases {
        assert_eq!(
            resolve(file_name, source, "target", 1),
            expected_line,
            "{file_name}"
        );
    }
}

#[test]
fn leading_bom_is_hidden_from_row_zero_columns_and_text() {
    for file_name in ["demo.ts", "demo.unknown"] {
        let source = "\u{feff}const target = 1;\nconst other = target;\n";
        let hit = resolve_position_in_file_content(file_name, source, &fuzzy("target", Some(1)))
            .expect("resolves");
        assert_eq!(
            (hit.position.line, hit.position.character),
            (0, 6),
            "{file_name}"
        );
        assert_eq!(hit.line_content, "const target = 1;", "{file_name}");
        // Later rows are unaffected by the BOM.
        let hit = resolve_position_in_file_content(file_name, source, &fuzzy("target", Some(2)))
            .expect("resolves");
        assert_eq!(
            (hit.position.line, hit.position.character),
            (1, 14),
            "{file_name}"
        );
    }
    // A symbol at column 0 right after the BOM.
    let hit =
        resolve_position_in_file_content("demo.py", "\u{feff}target = 1\n", &fuzzy("target", None))
            .expect("resolves");
    assert_eq!((hit.position.line, hit.position.character), (0, 0));
}
