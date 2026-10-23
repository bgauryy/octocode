use super::*;
use std::time::Duration;

#[test]
fn expired_signature_deadline_does_not_return_partial_boundaries() {
    let cfg = LangExtractConfig {
        language: tree_sitter_rust::LANGUAGE.into(),
        body_query: "(function_item body: (block) @body)",
    };
    assert!(
        extract_with_limits(
            "fn f() {\n work();\n}\n",
            &cfg,
            Instant::now(),
            65_536,
            false
        )
        .is_none()
    );
    assert!(
        parse_before(
            "fn healthy() {}",
            &cfg.language,
            Instant::now() + Duration::from_secs(2)
        )
        .is_some(),
        "an interrupted parse must not poison worker-local parser scratch state"
    );
}

#[test]
fn exhausted_signature_query_does_not_return_partial_boundaries() {
    let cfg = LangExtractConfig {
        language: tree_sitter_rust::LANGUAGE.into(),
        body_query: "(block (expression_statement)* @body (expression_statement) @body)",
    };
    let source = "fn f() {\n one();\n two();\n three();\n four();\n}\n";
    assert!(
        extract_with_limits(
            source,
            &cfg,
            Instant::now() + Duration::from_secs(2),
            1,
            false
        )
        .is_none()
    );
    assert!(extract(source, &cfg).is_some());
}

#[test]
fn outline_collapses_top_level_import_runs_to_one_line() {
    let cfg = LangExtractConfig {
        language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        body_query: "(function_declaration body: (statement_block) @body)",
    };
    let source = "import a from 'a';\n// note\nimport {\n  b,\n} from 'b';\nimport c from 'c';\n\nexport function f() {\n  return a;\n}\nimport late from 'late';\n";
    let outline = extract_outline(source, &cfg).expect("outline");
    assert_eq!(
        outline,
        vec![
            (1, "import … (lines 1-6)".to_owned()),
            (7, String::new()),
            (8, "export function f() {".to_owned()),
            (11, "import late from 'late';".to_owned()),
        ]
    );
    // The boundary view keeps every line.
    assert_eq!(extract(source, &cfg).expect("lines").len(), 9);
}

fn outline_of(ext: &str, source: &str) -> Vec<(usize, String)> {
    let entry = super::super::languages::find_entry(ext).expect("grammar");
    let cfg = LangExtractConfig {
        language: entry.language.clone(),
        body_query: entry.body_query,
    };
    extract_outline(source, &cfg).expect("outline")
}

fn lines(rows: &[(usize, &str)]) -> Vec<(usize, String)> {
    rows.iter().map(|(n, t)| (*n, (*t).to_owned())).collect()
}

/// A multi-line signature is one outline line (its first, marked with
/// ` …`): the declaration head's line range reads the rest. Single-line
/// signatures, and the boundary view, are untouched.
#[test]
fn outline_collapses_multi_line_signatures_to_their_first_line() {
    let rust = "pub fn of(\n    query: &Query,\n    root: &Path,\n) -> Self {\n    work();\n}\n\nfn one(a: u8) -> u8 {\n    a\n}\n";
    assert_eq!(
        outline_of("rs", rust),
        lines(&[(1, "pub fn of( …"), (7, ""), (8, "fn one(a: u8) -> u8 {")])
    );
    let python = "class A:\n    def run(self,\n            x,\n            y):\n        work()\n\n    def one(self): return 1\n";
    assert_eq!(
        outline_of("py", python),
        lines(&[
            (1, "class A:"),
            (2, "    def run(self, …"),
            (6, ""),
            (7, "    def one(self): return 1")
        ])
    );
    let ts = "export const make = (\n  a: string,\n  b: number,\n): void => {\n  go();\n};\n";
    assert_eq!(
        outline_of("ts", ts),
        lines(&[(1, "export const make = ( …")])
    );
    let cfg = LangExtractConfig {
        language: tree_sitter_rust::LANGUAGE.into(),
        body_query: "(function_item body: (block) @body)",
    };
    // Rows 1-4 (the whole signature), the blank row and `fn one`.
    assert_eq!(extract(rust, &cfg).expect("boundary").len(), 6);
}

/// Stacked or multi-row annotations shrink to their first row (marked
/// ` …`), one annotation row stays whole, and closing-delimiter rows
/// drop: the outline lists items, their heads' line ranges read the rest.
#[test]
fn outline_collapses_annotation_runs_and_drops_closing_rows() {
    let rust = "#[doc = \"A type.\"]\n#[derive(\n    Clone,\n    Debug,\n)]\npub struct S {\n    #[doc = \"Field.\"]\n    #[serde(default)]\n    pub a: u8,\n    #[serde(rename = \"b\")]\n    pub b: u8,\n}\nimpl S {\n    fn f(&self) {\n        work();\n    }\n}\n";
    assert_eq!(
        outline_of("rs", rust),
        lines(&[
            (1, "#[doc = \"A type.\"] …"),
            (6, "pub struct S {"),
            (7, "    #[doc = \"Field.\"] …"),
            (9, "    pub a: u8,"),
            (10, "    #[serde(rename = \"b\")]"),
            (11, "    pub b: u8,"),
            (13, "impl S {"),
            (14, "    fn f(&self) {"),
        ])
    );
    let python = "@app.route(\"/x\")\n@login_required\ndef view():\n    return 1\n\n@property\ndef one(self):\n    return 1\n";
    assert_eq!(
        outline_of("py", python),
        lines(&[
            (1, "@app.route(\"/x\") …"),
            (3, "def view():"),
            (5, ""),
            (6, "@property"),
            (7, "def one(self):"),
        ])
    );
}

/// A module or class docstring is its first text line (` …` marks the
/// cut), not the whole prose; code and single-line docstrings stay.
#[test]
fn outline_keeps_the_first_docstring_line() {
    let python = "# License\n\"\"\"\nRead and write ZIP files.\n\nDetails that run on.\n\"\"\"\nimport os\n\nclass A:\n    \"\"\"Class summary.\n\n    More.\n    \"\"\"\n    X = 1\n\nclass B:\n    \"\"\"One line.\"\"\"\n    def f(self):\n        work()\n";
    assert_eq!(
        outline_of("py", python),
        lines(&[
            (1, "# License"),
            (2, "\"\"\""),
            (3, "Read and write ZIP files. …"),
            (7, "import os"),
            (8, ""),
            (9, "class A:"),
            (10, "    \"\"\"Class summary. …"),
            (14, "    X = 1"),
            (15, ""),
            (16, "class B:"),
            (17, "    \"\"\"One line.\"\"\""),
            (18, "    def f(self):"),
        ])
    );
}

#[test]
fn helper_captures_do_not_remove_signature_lines() {
    let source = "def keep():\n    work()\n";
    let cfg = LangExtractConfig {
        language: tree_sitter_python::LANGUAGE.into(),
        body_query: "(function_definition body: (block) @body) @_function",
    };
    let outline = extract(source, &cfg).expect("outline retains the signature");
    assert_eq!(outline, vec![(1, "def keep():".to_owned())]);
}

#[test]
fn rust_query_cursor_filters_builtin_text_predicates() {
    let source = "def strip():\n    removed = 1\n\ndef keep():\n    preserved = 2\n";
    for query_src in [
        "((function_definition name: (identifier) @_name body: (block) @body) (#eq? @_name \"strip\"))",
        "((function_definition name: (identifier) @_name body: (block) @body) (#any-of? @_name \"strip\" \"other\"))",
        "((function_definition name: (identifier) @_name body: (block) @body) (#match? @_name \"^strip$\"))",
    ] {
        let language = tree_sitter_python::LANGUAGE.into();
        let query = cached_query(&language, query_src).expect("valid built-in predicate");
        assert!(query.general_predicates(0).is_empty());
        let lines = extract(
            source,
            &LangExtractConfig {
                language,
                body_query: query_src,
            },
        )
        .expect("outline");
        let outline = lines
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(outline.contains("def strip"));
        assert!(!outline.contains("removed"), "{query_src}: {outline}");
        assert!(outline.contains("preserved"), "{query_src}: {outline}");
    }
}

#[test]
fn unsupported_predicates_preserve_source_by_rejecting_the_query() {
    let language = tree_sitter_python::LANGUAGE.into();
    for query_src in [
        "((function_definition body: (block) @body) (#unsupported? @body))",
        "((function_definition body: (block) @body) (#is? local))",
    ] {
        assert!(cached_query(&language, query_src).is_none());
    }
}

#[test]
fn cached_query_reuses_the_same_compiled_query_across_calls() {
    // Repeated calls for the same (language, body_query) must return the
    // same compiled Query (Arc::ptr_eq), not recompile it — the fix for
    // `extractor.rs` recompiling a static per-language query on every
    // file scanned.
    let lang: Language = tree_sitter_python::LANGUAGE.into();
    let query_src = r#"(function_definition body: (block) @body)"#;

    let first = cached_query(&lang, query_src).expect("query should compile");
    let second = cached_query(&lang, query_src).expect("query should compile");

    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "expected the second call to reuse the cached Query, got a distinct instance"
    );
}

#[test]
fn cached_query_returns_none_for_an_invalid_query_without_poisoning_the_cache() {
    let lang: Language = tree_sitter_python::LANGUAGE.into();
    // Malformed query text — not a valid tree-sitter S-expression.
    let bad_query = "(this is not valid";
    assert!(cached_query(&lang, bad_query).is_none());

    // The cache must still work for a valid query afterward.
    let good_query = r#"(function_definition body: (block) @body)"#;
    assert!(cached_query(&lang, good_query).is_some());
}
