use super::*;

#[test]
fn code_occurrences_skip_comments_and_strings() {
    let source = "//! Links [`Continuation`].\n// Continuation here\nfn f(c: Continuation) {\n    let s = \"Continuation\";\n    c.next();\n}\n";
    assert_eq!(
        crate::lsp::resolver::code_occurrence_lines("lib.rs", source, "Continuation"),
        Some(vec![2])
    );
    assert_eq!(
        crate::lsp::resolver::code_occurrence_lines(
            "lib.rs",
            "// only Continuation\n",
            "Continuation"
        ),
        Some(Vec::new())
    );
    assert_eq!(
        crate::lsp::resolver::code_occurrence_lines("notes.unknownext", source, "f"),
        None
    );
}

#[test]
fn line_index_breaks_on_crlf_lf_and_lone_cr() {
    let text = "a\r\nb\rc\nd";
    let index = LineIndex::new(text);
    assert_eq!(index.lines(text), ["a", "b", "c", "d"]);
    assert_eq!(index.content_len(), 4);
    assert_eq!(index.position_of(text.find('c').unwrap()), (2, 0));
    assert_eq!(index.position_of(text.find('d').unwrap()), (3, 0));

    // A trailing break yields an empty last line that holds no content.
    let text = "x\r\ny\r";
    let index = LineIndex::new(text);
    assert_eq!(index.lines(text), ["x", "y", ""]);
    assert_eq!(index.len(), 3);
    assert_eq!(index.content_len(), 2);
    assert_eq!(LineIndex::new("").lines(""), [""]);
    assert_eq!(LineIndex::new("").content_len(), 1);
}

#[test]
fn an_out_of_range_line_counts_the_lines_the_file_has() {
    // 29 lines, each ending in a break: the empty text after the last
    // break is no line.
    let source = "let a = 1;\n".repeat(29);
    let error = crate::lsp::resolver::resolve_position_from_content(source, fuzzy("a", Some(40)))
        .expect_err("line 40 is past the end");
    assert!(error.to_string().contains("(file has 29 lines)"), "{error}");
    let error = crate::lsp::resolver::resolve_position_from_content(
        "let a = 1;".into(),
        fuzzy("a", Some(3)),
    )
    .expect_err("line 3 is past the end");
    assert!(error.to_string().contains("(file has 1 lines)"), "{error}");
}

#[test]
fn lexical_resolution_counts_crlf_and_lone_cr_as_line_breaks() {
    for source in [
        "let a = 1;\r\nlet b = 2;\r\nfn target() {}\r\n",
        "let a = 1;\rlet b = 2;\rfn target() {}\r",
        "let a = 1;\nlet b = 2;\r\nfn target() {}\r",
    ] {
        let hit = crate::lsp::resolver::resolve_position_from_content(
            source.to_owned(),
            fuzzy("target", None),
        )
        .expect("target resolves");
        assert_eq!(hit.position.line, 2, "{source:?}");
        assert_eq!(hit.position.character, 3, "{source:?}");
        assert_eq!(hit.line_content, "fn target() {}");
    }
}

#[test]
fn grammar_resolution_counts_lone_cr_as_a_line_break() {
    // Tree-sitter rows count only `\n`; with lone `\r` breaks the whole
    // file is one tree-sitter row, but the server sees three lines.
    let source = "const a = 1;\rconst b = 2;\rfunction target() {}\r";
    let hit = resolve_position_in_file_content("demo.ts", source, &fuzzy("target", None))
        .expect("target resolves");
    assert_eq!(hit.position.line, 2);
    assert_eq!(hit.position.character, 9);
    assert_eq!(hit.found_at_line, 3);
    assert_eq!(hit.line_content, "function target() {}");

    let crlf = "const a = 1;\r\nfunction target() {}\r\n";
    let hit = resolve_position_in_file_content("demo.ts", crlf, &fuzzy("target", Some(2)))
        .expect("target resolves");
    assert_eq!((hit.position.line, hit.position.character), (1, 9));
}

#[test]
fn grammar_resolution_reports_utf16_columns_after_non_ascii_and_emoji() {
    // "é" is 2 UTF-8 bytes / 1 UTF-16 unit; "😀" is 4 bytes / 2 units.
    let source = "const a = 1;\r\nconst s = 'é😀'; const target = 2;\r\n";
    let hit = resolve_position_in_file_content("demo.ts", source, &fuzzy("target", Some(2)))
        .expect("target resolves");
    assert_eq!(hit.position.line, 1);
    let expected = "const s = 'é😀'; const ".encode_utf16().count() as u32;
    assert_eq!(hit.position.character, expected);

    let lexical = crate::lsp::resolver::resolve_position_from_content(
        "x\ré😀 target\n".to_owned(),
        fuzzy("target", Some(2)),
    )
    .expect("lexical target");
    assert_eq!((lexical.position.line, lexical.position.character), (1, 4));
}

/// An `occurrence` past the line's occurrences is an error naming the
/// count, in the grammar walk and the lexical scan alike; it never
/// answers for another occurrence.
#[test]
fn an_order_hint_past_the_lines_occurrences_is_an_error() {
    for file in ["demo.ts", "demo.unknown"] {
        let result = resolve_position_in_file_content(
            file,
            "const other = 1;\nconst a = target + target;\n",
            &LspFuzzyPosition {
                symbol_name: "target".to_owned(),
                line_hint: Some(2),
                order_hint: Some(3),
            },
        );
        let error = match result {
            Err(error) => error,
            Ok(hit) => panic!("{file}: resolved {hit:?}"),
        };
        assert!(
            error.reason.contains("2 occurrence(s)"),
            "{file}: {}",
            error.reason
        );
    }
}

#[test]
fn oversized_position_source_returns_limit_instead_of_a_symbol() {
    let source = format!(
        "const target = 1;\n{}",
        " ".repeat(crate::lsp::MAX_LSP_SOURCE_BYTES as usize)
    );
    let result = resolve_position_in_file_content(
        "demo.ts",
        &source,
        &LspFuzzyPosition {
            symbol_name: "target".to_owned(),
            line_hint: Some(1),
            order_hint: None,
        },
    );
    let Err(error) = result else {
        panic!("oversized source must be bounded")
    };
    assert!(
        error.reason.contains("[lspSourceTooLarge]"),
        "{}",
        error.reason
    );
}

#[test]
fn expired_position_budget_does_not_fall_back_to_a_lexical_hit() {
    for file in ["demo.ts", "demo.unknown"] {
        let result = crate::lsp::resolver::resolve_position_before(
            file,
            "const target = 1;",
            &LspFuzzyPosition {
                symbol_name: "target".to_owned(),
                line_hint: Some(1),
                order_hint: None,
            },
            std::time::Instant::now(),
        );
        let Err(error) = result else {
            panic!("expired analysis must fail")
        };
        assert!(error.reason.contains("[lspPositionTimeout]"));
    }
}

#[test]
fn expired_walk_budget_discards_partial_candidates() {
    let source = "const target = 1;";
    let spec = crate::lsp::resolver::grammar_for_file("demo.ts").unwrap();
    let tree = spec
        .parse_before(source, crate::signatures::extractor::whole_file_deadline())
        .unwrap();
    let mut candidates = Vec::new();
    let result = crate::lsp::resolver::collect_symbol_candidates(
        tree.root_node(),
        source,
        &crate::lsp::resolver::LineIndex::new(source),
        "target",
        &mut candidates,
        std::time::Instant::now(),
    );
    assert!(
        result
            .expect_err("walk budget must fail")
            .reason
            .contains("[lspPositionTimeout]")
    );
    assert!(candidates.is_empty());
}

#[test]
fn deeply_nested_position_walk_is_bounded_without_recursive_stack_growth() {
    let source = format!(
        "const target = {}1{};",
        "(".repeat(10_000),
        ")".repeat(10_000)
    );
    let result = crate::lsp::resolver::resolve_position_in_file_content(
        "demo.ts",
        &source,
        &LspFuzzyPosition {
            symbol_name: "target".to_owned(),
            line_hint: Some(1),
            order_hint: None,
        },
    );
    match result {
        Ok(hit) => assert_eq!(hit.position.character, 6),
        Err(error) => assert!(
            error.reason.contains("[lspPositionTimeout]"),
            "{}",
            error.reason
        ),
    }
}
