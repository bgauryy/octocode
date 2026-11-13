use super::*;

#[test]
fn raw_identifiers_are_exact_code_occurrences_under_either_spelling() {
    let source = "fn r#type() {}\nfn call() { r#type(); }\nfn type_suffix() {}\n";
    for name in ["type", "r#type"] {
        assert_eq!(
            super::super::code_occurrence_lines("lib.rs", source, name),
            Some(vec![0, 1]),
            "{name}"
        );
    }
    assert_eq!(
        super::super::code_occurrence_lines("lib.rs", source, "other_type"),
        Some(Vec::new())
    );
}

#[test]
fn raw_identifier_anchors_prefer_the_declaration_without_changing_source_positions() {
    let source = "fn call() { r#target(); }\nfn r#target() {}\n";
    for name in ["target", "r#target"] {
        let hit = resolve_position_in_file_content("lib.rs", source, &fuzzy(name, None))
            .expect("raw declaration");
        assert_eq!(hit.found_at_line, 2, "{name}: {hit:?}");
        let declaration = source.lines().nth(1).expect("declaration");
        assert_eq!(
            hit.position.character as usize,
            declaration.find(name).expect("source spelling"),
            "{name}: {hit:?}"
        );
    }
}

#[test]
fn raw_and_ordinary_rust_spellings_name_the_same_identifier() {
    let source = "fn target() {}\nfn call() { r#target(); target(); }\n";
    for name in ["target", "r#target"] {
        assert_eq!(
            super::super::code_occurrence_lines("lib.rs", source, name),
            Some(vec![0, 1]),
            "{name}"
        );
    }
    assert_eq!(
        super::super::code_occurrence_lines(
            "lib.rs",
            "fn target_suffix() {}\nconst TEXT: &str = \"r#target\";\n",
            "target"
        ),
        Some(Vec::new()),
        "neither a longer name nor a string is the identifier"
    );
}

#[test]
fn raw_prefix_normalization_is_rust_specific_and_never_creates_an_empty_name() {
    use super::super::canonical_symbol_name;
    assert_eq!(canonical_symbol_name(Some("rust"), "r#type"), "type");
    assert_eq!(canonical_symbol_name(Some("rust"), "ordinary"), "ordinary");
    assert_eq!(canonical_symbol_name(Some("rust"), "r#"), "r#");
    for language in [None, Some("typescript"), Some("csharp")] {
        assert_eq!(canonical_symbol_name(language, "r#type"), "r#type");
    }
}

#[test]
fn raw_identifier_code_ranges_keep_every_real_site_and_exclude_text_mentions() {
    let source = "fn r#type() {}\nfn call() { r#type(); let label = \"r#type\"; r#type(); } // r#type\n/* r#type */\n";
    for name in ["type", "r#type"] {
        let occurrences =
            super::super::code_occurrences("lib.rs", source, name).expect("clean source");
        let ranges = occurrences.ranges();
        assert_eq!(ranges.len(), 3, "{name}: {ranges:?}");
        assert_eq!(occurrences.lines, vec![0, 1], "{name}");
        let index = LineIndex::new(source);
        for range in ranges {
            let start = index.position_to_byte(source, range.start.line, range.start.character);
            let end = index.position_to_byte(source, range.end.line, range.end.character);
            assert_eq!(&source[start as usize..end as usize], "r#type");
        }
    }
}

#[test]
fn code_occurrence_ranges_exclude_character_regex_and_jsx_literals() {
    for (file, source, name) in [
        (
            "lib.rs",
            "fn x() {}\nfn call() { let label = 'x'; x(); }\n",
            "x",
        ),
        (
            "index.tsx",
            "function target() {}\nconst view = <div>target</div>;\nconst pattern = /target/;\ntarget();\n",
            "target",
        ),
    ] {
        let occurrences = super::super::code_occurrences(file, source, name).expect("clean source");
        assert_eq!(occurrences.ranges().len(), 2, "{file}");
    }
}
