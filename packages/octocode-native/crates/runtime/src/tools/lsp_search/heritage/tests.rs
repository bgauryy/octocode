use super::*;

mod hedge;

/// Callers hold canonical absolute paths; a node's URI needs one.
fn absolute(file: &str) -> String {
    std::env::temp_dir()
        .join(file)
        .to_string_lossy()
        .into_owned()
}

fn bases(content: &str, file: &str, line: u32) -> Vec<(String, String, u32, u32)> {
    let file = &absolute(file);
    let document = type_document(content, file).expect("graph facts");
    let declaration = document
        .declarations
        .iter()
        .filter(|declaration| {
            declaration.range.start.line <= line
                && line <= declaration.range.end.line
                && TYPE_DECLARATION_KINDS.contains(&declaration.kind.as_str())
        })
        .min_by_key(|declaration| declaration.range.end.line - declaration.range.start.line)
        .expect("type declaration");
    let anchor = &declaration.selection_range.start;
    let node = type_node(&document, file, anchor.line, anchor.character).expect("type node");
    direct_bases(&document, content, file, &node)
        .expect("direct heritage")
        .into_iter()
        .map(|b| (b.name, b.relation, b.line, b.character))
        .collect()
}

#[test]
fn a_typescript_class_names_its_extends_and_implements_clauses_with_positions() {
    let source = "interface Run {}\nclass Base {}\nexport class Child extends Base implements Run {\n  go() {}\n}\n";
    let found = bases(source, "a.ts", 3);
    assert_eq!(
        found,
        vec![
            ("Base".into(), "extends".into(), 2, 27),
            ("Run".into(), "implements".into(), 2, 43),
        ]
    );
}

#[test]
fn the_innermost_declaration_holding_the_line_answers() {
    let source = "class A {}\nclass B extends A {\n  m() { return 1; }\n}\n";
    assert_eq!(bases(source, "a.ts", 2).len(), 1);
    assert!(bases(source, "a.ts", 0).is_empty(), "A has no bases");
}

#[test]
fn a_python_class_reads_dotted_bases_at_their_last_segment() {
    let source = "class Child(pkg.Base, Mixin):\n    pass\n";
    assert_eq!(
        bases(source, "a.py", 0),
        vec![
            ("pkg.Base".into(), "extends".into(), 0, 16),
            ("Mixin".into(), "extends".into(), 0, 22),
        ]
    );
}

#[test]
fn a_recovered_parse_proves_no_heritage_but_the_syntax_only_caveat_does_not_block() {
    let clean = type_document("class Child(Base):\n    pass\n", &absolute("a.py"))
        .expect("a clean tree-sitter parse answers");
    assert!(
        clean
            .diagnostics
            .iter()
            .any(|note| note.code == GraphFactCode::SyntaxOnly)
    );
    let broken = type_document("class Child(Base:\n    pass\n", &absolute("a.py"))
        .expect_err("a recovered parse may miss a clause");
    assert!(
        broken.starts_with("Type heritage parse is incomplete"),
        "{broken}"
    );
    assert!(!broken.contains("syntax-only"), "{broken}");
}

/// rust-analyzer locates a trait implementation at its self type: that
/// impl is the subtype node, and its base is the trait it names before
/// the self type, past a bound that spells the same trait.
#[test]
fn a_rust_trait_impl_is_a_node_whose_base_is_its_trait() {
    let source =
        "trait Scan {}\nstruct Foo<T>(T);\nimpl<T: Scan> Scan for Foo<T> {}\nimpl<T> Foo<T> {}\n";
    let file = &absolute("lib.rs");
    let document = type_document(source, file).expect("graph facts");
    let node = impl_node(&document, file, 2, 23).expect("trait impl");
    assert_eq!(
        (node["name"].as_str(), node["kind"].as_str()),
        (Some("Foo"), Some("impl"))
    );
    let found = direct_bases(&document, source, file, &node)
        .expect("trait clause")
        .into_iter()
        .map(|b| (b.name, b.relation, b.line, b.character))
        .collect::<Vec<_>>();
    assert_eq!(found, vec![("Scan".into(), "implements".into(), 2, 14)]);
    assert!(impl_node(&document, file, 3, 8).is_none(), "inherent impl");
    assert!(impl_node(&document, file, 1, 7).is_none(), "struct");
}

#[test]
fn a_whole_word_match_skips_longer_identifiers() {
    assert_eq!(
        bases("class XBase extends Base {}", "a.ts", 0),
        vec![("Base".into(), "extends".into(), 0, 20)]
    );
}

#[test]
fn heritage_anchors_keep_same_line_declarations_and_utf16_columns_distinct() {
    let content = "const emoji = \"😀\"; class A extends ns.Base {} class B extends Other {}";
    let file = &absolute("a.ts");
    let document = type_document(content, file).expect("graph facts");
    for (name, base) in [("A", "ns.Base"), ("B", "Other")] {
        let offset = content.find(&format!("class {name}")).expect("declaration") + "class ".len();
        let column = u32::try_from(content[..offset].encode_utf16().count()).expect("column");
        let node = type_node(&document, file, 0, column).expect("exact declaration");
        let bases = direct_bases(&document, content, file, &node).expect("direct heritage");
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].name, base);
        let leaf = base.rsplit('.').next().expect("leaf");
        let offset = content.find(base).expect("base") + base.len() - leaf.len();
        assert_eq!(
            bases[0].character,
            u32::try_from(content[..offset].encode_utf16().count()).expect("column")
        );
    }
}

#[test]
fn only_subtypes_and_supertypes_have_a_fallback() {
    assert_eq!(
        fallback_provider("subtypes"),
        Some("implementationProvider")
    );
    assert_eq!(fallback_provider("supertypes"), Some("definitionProvider"));
    assert_eq!(fallback_provider("callers"), None);
}

/// `(line, start, end, content)` of each declaration a source's heritage
/// clauses name `base` in, zero-based.
fn subtypes(content: &str, file: &str, base: &str) -> Vec<(u32, u32, u32, String)> {
    subtype_snippets(content, file, base)
        .into_iter()
        .map(|found| {
            (
                found.range.start.line,
                found.range.start.character,
                found.range.end.character,
                found.content,
            )
        })
        .collect()
}

#[test]
fn subtypes_are_the_declarations_whose_heritage_names_the_base() {
    let source = "interface Base {}\nclass A extends Base {}\nclass B implements Other, Base {}\ninterface C extends ns.Base {}\nclass D extends Other {}\nclass Base2 extends Base {}\n";
    assert_eq!(
        subtypes(source, "/work/a.ts", "Base"),
        // Each row carries its declaration line, as a server location does.
        vec![
            (1, 6, 7, "class A extends Base {}".into()),
            (2, 6, 7, "class B implements Other, Base {}".into()),
            (3, 10, 11, "interface C extends ns.Base {}".into()),
            (5, 6, 11, "class Base2 extends Base {}".into()),
        ]
    );
    assert!(subtypes(source, "/work/a.ts", "Missing").is_empty());
}

/// A file a tree-sitter grammar reads gives its subtypes from the parts that
/// spell the base: the same rows, without the whole file's facts.
#[test]
fn a_grammar_file_scans_without_its_whole_facts() {
    let source = "class Base {}\nclass A extends Base implements Other {}\nclass B { void f() { Base b = null; } }\ninterface C extends Base {}\n";
    let file = absolute("subtype-scan-parses/A.java");
    assert_eq!(
        subtypes(source, &file, "Base"),
        vec![
            (1, 6, 7, "class A extends Base implements Other {}".into()),
            (3, 10, 11, "interface C extends Base {}".into()),
        ]
    );
    assert_eq!(
        crate::tools::declarations::parses::facts_of(&file),
        0,
        "whole-file facts parses"
    );
}

/// A scan reads each file's subtypes once per text: a repeat scan reads the
/// kept documents, and a Go file, whose heritage only embeds, is not parsed.
#[test]
fn a_repeat_scan_parses_nothing_and_go_files_never() {
    let dir = tempfile::tempdir().expect("dir");
    let root = std::fs::canonicalize(dir.path()).expect("root");
    let java = root.join("A.java");
    let go = root.join("a.go");
    std::fs::write(&java, "class A implements Querier {}\n").expect("java");
    std::fs::write(&go, "package a\n\ntype S struct {\n\tQuerier\n}\n").expect("go");
    let (java, go) = (
        java.to_string_lossy().into_owned(),
        go.to_string_lossy().into_owned(),
    );
    let files = BTreeSet::from([java.clone(), go.clone()]);
    let parses = |file: &str| {
        ["namedHeritage", "graphFacts"]
            .map(|analysis| crate::tools::declarations::parses::of(analysis, file))
    };
    let first = scan_subtypes(&files, "Querier", &|| false);
    assert_eq!(first.0.len(), 1, "the Java implementation");
    assert!(first.0[0].uri.ends_with("/A.java"));
    assert!(first.1.is_empty());
    assert_eq!(parses(&java), [1, 0]);
    assert_eq!(parses(&go), [0, 0], "Go embeds name no supertype");
    let again = scan_subtypes(&files, "Querier", &|| false);
    let rows = |found: &[LspCodeSnippet]| {
        found
            .iter()
            .map(|snippet| serde_json::to_value(snippet).expect("row"))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&again.0), rows(&first.0));
    assert_eq!(again.1, first.1);
    assert_eq!(parses(&java), [1, 0], "a repeat reads the kept document");
}

#[test]
fn only_a_type_declaration_anchors_a_subtype_scan() {
    let source = "export interface IDisposable {\n  dispose(): void;\n}\nexport class Box {\n  run(): void {}\n}\nexport function make(): void {}\n";
    let on = |line: u32, name: &str| anchors_type_declaration(source, "/work/a.ts", line, name);
    assert!(on(0, "IDisposable"), "an interface");
    assert!(on(3, "Box"), "a class");
    assert!(
        !on(1, "dispose"),
        "an interface member has no subtypes to scan"
    );
    assert!(
        !on(4, "run"),
        "a method is implemented per override, not per heritage"
    );
    assert!(!on(6, "make"), "a function");
    assert!(!on(0, "Other"), "another name on the line");
    assert!(!on(8, "Box"), "a line past the file");
}

#[test]
fn a_base_spelled_outside_a_heritage_clause_names_no_subtype() {
    let source = "class Base {}\nconst x: Base = new Base();\n// class Z extends Base {}\n";
    assert!(subtypes(source, "/work/a.ts", "Base").is_empty());
}

/// The warning names as many unread files as the lead lists one row
/// each, and counts the rest.
#[test]
fn the_unread_warning_names_a_few_and_counts_the_rest() {
    let unread = (1..=7).map(|n| format!("src/f{n}.ts")).collect::<Vec<_>>();
    let warning = describe_unread(&unread);
    assert!(warning.contains("7 file(s)"), "{warning}");
    assert!(warning.contains("src/f5.ts and 2 more."), "{warning}");
    assert!(!warning.contains("src/f6.ts"), "{warning}");
    assert!(describe_unread(&unread[..1]).ends_with("missing: src/f1.ts."));
}

/// The scan names every file it could not read, so the gap in a
/// heritage answer can be followed.
#[test]
fn the_scan_names_the_files_it_could_not_read() {
    let dir = tempfile::tempdir().expect("workspace");
    let readable = dir.path().join("a.ts");
    std::fs::write(&readable, "class A extends Base {}\n").expect("source");
    let missing = dir.path().join("gone.ts").display().to_string();
    let files = BTreeSet::from([readable.display().to_string(), missing.clone()]);
    let (found, unread) = scan_subtypes(&files, "Base", &|| false);
    assert_eq!(found.len(), 1);
    assert_eq!(unread, vec![missing]);
}

#[test]
fn a_syntax_fallback_note_discloses_direct_unverified_relations() {
    let failure = LspFailure::invalid_query("x");
    assert!(describe_scan(&failure, "Base", 3, "subtypes").contains("direct subtypes only"));
    let implementations = describe_scan(&failure, "Base", 3, "implementation");
    assert!(
        implementations.contains("direct implementations only")
            && implementations.contains("run implementation on each"),
        "{implementations}"
    );
}

/// A server that cannot answer (timeout, crash, rejected request) leaves
/// the syntax scan the cheaper labelled answer; a cancelled request or a
/// refused path does not.
#[test]
fn a_failed_implementation_request_has_the_syntax_scan_as_fallback() {
    use super::super::failure::LspFailure;
    use octocode_engine::error::{Error, ErrorKind};
    let failure = |kind| LspFailure::from_engine(&Error::with_kind(kind, "x"));
    assert!(implementation_failure_has_fallback(&failure(
        ErrorKind::Timeout
    )));
    assert!(implementation_failure_has_fallback(&failure(
        ErrorKind::ConnectionClosed
    )));
    assert!(implementation_failure_has_fallback(&failure(
        ErrorKind::Other
    )));
    assert!(!implementation_failure_has_fallback(
        &LspFailure::invalid_query("x")
    ));
}

/// tokio's `AsyncRead` file: a `macro_rules!` expanded inside an impl leaves
/// only a linking caveat (its generated imports are unknown). Every parsed
/// declaration and its own heritage stay whole, so the trait still anchors
/// a hierarchy and the impl still names its trait.
#[test]
fn an_unexpanded_rust_macro_is_no_heritage_gap() {
    let file = &absolute("async_read.rs");
    let content = "pub trait AsyncRead {}\n\
        macro_rules! deref_async_read { () => { fn f() {} }; }\n\
        impl<T: AsyncRead> AsyncRead for Box<T> { deref_async_read!(); }\n";
    let document = type_document(content, file).expect("a linking caveat is no gap");
    assert!(
        document
            .diagnostics
            .iter()
            .any(|note| note.code == GraphFactCode::RustMacroExpansion),
        "the fixture carries the macro caveat"
    );
    let node = type_node(&document, file, 0, 10).expect("the trait anchors");
    assert_eq!(node["name"], "AsyncRead");
    let implementation = impl_node(&document, file, 2, 33).expect("the trait impl");
    let bases = direct_bases(&document, content, file, &implementation).expect("its heritage");
    assert_eq!(bases.len(), 1);
}
