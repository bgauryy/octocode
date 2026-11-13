use super::*;

#[cfg(any(feature = "tree-sitter-c-sharp", feature = "tree-sitter-scala"))]
#[test]
fn escaped_identifier_names_keep_full_source_ranges_and_accept_both_spellings() {
    for (file, source, canonical, spelling) in [
        #[cfg(feature = "tree-sitter-c-sharp")]
        (
            "Demo.cs",
            "class Demo {\n    void @class() {}\n    void Use() { @class(); }\n}\n",
            "class",
            "@class",
        ),
        #[cfg(feature = "tree-sitter-scala")]
        (
            "Demo.scala",
            "object Demo {\n  def `type`(): Unit = ()\n  def use(): Unit = { `type`() }\n}\n",
            "type",
            "`type`",
        ),
    ] {
        for name in [canonical, spelling] {
            let occurrences =
                super::super::code_occurrences(file, source, name).expect("clean source");
            assert_eq!(occurrences.ranges().len(), 2, "{file} {name}");
            let index = LineIndex::new(source);
            for range in occurrences.ranges() {
                let start = index.position_to_byte(source, range.start.line, range.start.character);
                let end = index.position_to_byte(source, range.end.line, range.end.character);
                assert_eq!(
                    &source[start as usize..end as usize],
                    spelling,
                    "{file} {name}"
                );
            }
            let hit = resolve_position_in_file_content(file, source, &fuzzy(name, Some(2)))
                .expect("declaration anchor");
            assert_eq!(hit.found_at_line, 2, "{file} {name}");
        }
    }
}

#[test]
fn javascript_identifier_sigils_are_part_of_identity() {
    let source =
        "function $target() {}\nclass Demo { #target() {} use() { this.#target(); $target(); } }\n";
    assert!(
        super::super::code_occurrences("index.js", source, "target")
            .expect("clean source")
            .ranges()
            .is_empty()
    );
    for name in ["$target", "#target"] {
        assert_eq!(
            super::super::code_occurrences("index.js", source, name)
                .expect("clean source")
                .ranges()
                .len(),
            2,
            "{name}"
        );
    }
}

#[test]
fn quoted_literal_anchors_and_lifetimes_keep_their_existing_resolution() {
    let source = "const label = \"target\";\n";
    let hit = resolve_position_in_file_content("index.ts", source, &fuzzy("target", Some(1)))
        .expect("literal anchor");
    assert_eq!(
        hit.position.character as usize,
        source.find("target").expect("literal")
    );
    assert!(
        super::super::code_occurrences("index.ts", source, "target")
            .expect("clean source")
            .ranges()
            .is_empty()
    );
    let source = "fn borrow<'a>(input: &'a str) -> &'a str { input }\n";
    for name in ["a", "'a"] {
        let occurrences =
            super::super::code_occurrences("lib.rs", source, name).expect("clean source");
        assert_eq!(occurrences.ranges().len(), 3, "{name}");
        let index = LineIndex::new(source);
        for range in occurrences.ranges() {
            let start = index.position_to_byte(source, range.start.line, range.start.character);
            let end = index.position_to_byte(source, range.end.line, range.end.character);
            assert_eq!(&source[start as usize..end as usize], "'a", "{name}");
        }
    }
}

#[test]
fn literal_boundaries_keep_declaration_names_and_interpolation_code() {
    for (file, source, name, count) in [
        ("a.go", "package p\nvar x = 'x'\n", "x", 1),
        // One-element expression lists wrap both the declared name and the
        // value: each side is judged by its own node.
        ("a.go", "package p\nfunc f() { x := 'x'; _ = x }\n", "x", 2),
        ("a.rs", "fn f() { let x = 'x'; let _ = x; }\n", "x", 2),
        ("A.java", "class A { char x = 'x'; char y = x; }\n", "x", 2),
        (
            "a.js",
            "class Box { \"run\"() {} }\nconst label = \"run\"; // run\nnew Box().run();\n",
            "run",
            2,
        ),
        (
            "a.js",
            "const target = 1; const text = `target ${target}`;\n",
            "target",
            2,
        ),
        (
            "a.py",
            "target = 1\ntext = f'target {target}'\n",
            "target",
            2,
        ),
        #[cfg(feature = "tree-sitter-scala")]
        (
            "a.scala",
            "object Demo { val target = 1; val symbol = 'target }\n",
            "target",
            1,
        ),
        #[cfg(feature = "tree-sitter-scala")]
        (
            "a.scala",
            "object Demo { val target = 1; val text = s\"target ${target}\" }\n",
            "target",
            2,
        ),
        #[cfg(feature = "tree-sitter-c-sharp")]
        (
            "a.cs",
            "class Demo { static int target = 1; string text = $\"target {target}\"; }\n",
            "target",
            2,
        ),
    ] {
        let occurrences = super::super::code_occurrences(file, source, name).expect("clean source");
        assert_eq!(occurrences.ranges().len(), count, "{file}: {source}");
    }
}
