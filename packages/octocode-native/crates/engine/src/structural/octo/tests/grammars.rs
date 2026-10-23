//! Grammar-specific pattern contexts: C family, CUDA, C#, Java, Go, Rust, JSX.

use super::*;

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn ast_audit_cpp_multi_capture_body_matches_statements() {
    let pattern = "int $NAME($$$ARGS) { $$$BODY }";
    for ext in ["cpp", "hpp", "cc", "cxx", "hh", "hxx"] {
        for (source, body) in [
            ("int demo(int x) {}", Vec::<&str>::new()),
            ("int demo(int x) { return x; }", vec!["return x;"]),
            (
                "int demo(int x) { int y = x; return y; }",
                vec!["int y = x;", "return y;"],
            ),
        ] {
            for matches in [
                run_pattern(source, ext, pattern),
                run_rule(source, ext, json!({"pattern": pattern})),
            ] {
                assert_eq!(matches.len(), 1, "{ext}: {source}");
                assert_eq!(matches[0].metavars["NAME"], ["demo"]);
                assert_eq!(matches[0].metavars["BODY"], body);
            }
        }
    }
    let matches = run_pattern("int x{1};", "cpp", "int x{$VALUE};");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["VALUE"], ["1"]);
}

/// `template <...>` wraps the declaration whose `{ $$$BODY }` C++ reads as
/// an initializer list; the body must resolve there as it does at the root.
#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_template_function_body_capture_matches_statements() {
    let source = "template <typename T>\nvoid demo(T x) { use(x); done(); }\n";
    for pattern in [
        "template <typename T> void demo(T x) { $$$BODY }",
        "template <typename $T> void $NAME($$$ARGS) { $$$BODY }",
        "template <$$$PARAMS> void $NAME($$$ARGS) { $$$BODY }",
    ] {
        for matches in [
            run_pattern(source, "cpp", pattern),
            run_rule(source, "cpp", json!({"pattern": pattern})),
        ] {
            assert_eq!(matches.len(), 1, "{pattern}");
            assert_eq!(
                matches[0].metavars["BODY"],
                ["use(x);", "done();"],
                "{pattern}"
            );
        }
    }
    // A template of a concrete body stays exact.
    assert!(
        run_pattern(
            source,
            "cpp",
            "template <typename T> void demo(T x) { other(); }"
        )
        .is_empty()
    );
}

/// A CUDA launch is a statement: top level, the grammar reshapes it
/// (`parameter_pack_expansion` with a MISSING `...`), so it compiles in
/// function-body context and matches launches with or without `;`.
#[cfg(feature = "tree-sitter-cuda")]
#[test]
fn cuda_launch_and_template_kernel_patterns_match_real_code() {
    let source = "template <typename T>\n__global__ void scale(T *data, int n) { data[0] = n; }\n\
                  void run(float *data, dim3 grid) {\n  scale<<<grid, 256>>>(data, 4);\n  \
                  other<<<1, 32, 0, stream>>>(data);\n}\n";
    for ext in ["cu", "cuh"] {
        for (pattern, launches) in [
            ("$K<<<$G, $B>>>($$$A);", 1),
            ("$K<<<$G, $B>>>($$$A)", 1),
            ("$K<<<$$$CONFIG>>>($$$A)", 2),
            ("scale<<<grid, 256>>>(data, 4);", 1),
        ] {
            for matches in [
                run_pattern(source, ext, pattern),
                run_rule(source, ext, json!({"pattern": pattern})),
            ] {
                assert_eq!(matches.len(), launches, "{ext}: {pattern}");
                assert!(
                    matches[0].text.starts_with("scale<<<grid, 256>>>"),
                    "{ext}: {pattern}"
                );
            }
        }
        let config = run_pattern(source, ext, "$K<<<$$$CONFIG>>>($$$A)");
        assert_eq!(
            config[1].metavars["CONFIG"],
            ["1", ",", "32", ",", "0", ",", "stream"]
        );
        for pattern in [
            "template <typename $T> __global__ void $NAME($$$ARGS) { $$$BODY }",
            "template <$$$P> __global__ void $NAME($$$ARGS) { $$$BODY }",
        ] {
            let matches = run_pattern(source, ext, pattern);
            assert_eq!(matches.len(), 1, "{ext}: {pattern}");
            assert_eq!(matches[0].metavars["NAME"], ["scale"]);
            assert_eq!(matches[0].metavars["BODY"], ["data[0] = n;"]);
        }
    }
}

/// Function-body context is a fallback for snippets that fail alone; a
/// top-level declaration keeps its own tree, and several statements still
/// fail as several top-level nodes.
#[cfg(feature = "tree-sitter-cuda")]
#[test]
fn cuda_statement_context_leaves_declarations_and_sequences_alone() {
    let source = "int width = 4;\nvoid run() { int width = 8; }\n";
    assert_eq!(run_pattern(source, "cu", "int width = $V;").len(), 2);
    let sequence =
        StructuralQuery::new(Some("a<<<1, 1>>>(); b<<<1, 1>>>();"), None).expect("query");
    let error = compile_matcher(&lang("cu"), &sequence)
        .err()
        .expect("two launches are two nodes")
        .to_string();
    assert!(error.contains("2 top-level nodes"), "{error}");
}

fn assert_fragment_context(ext: &str, source: &str, bare: &str, expected: &str) {
    for capture in ["VALUE", "X"] {
        let bare = bare.replace("$VALUE", &format!("${capture}"));
        let terminated = format!("{bare};");
        for pattern in [bare.as_str(), terminated.as_str()] {
            let rule = json!({"all": [{"pattern": pattern}, {"regex": "."}]});
            for matches in [
                run_pattern(source, ext, pattern),
                run_rule(source, ext, &rule),
            ] {
                assert_eq!(matches.len(), 1, "{ext}: {pattern}");
                assert_eq!(matches[0].text, expected, "{ext}: {pattern}");
                assert_eq!(matches[0].metavars[capture], ["value"]);
                let range = &matches[0].metavar_ranges[capture][0];
                assert_eq!(range.text, "value");
                assert_eq!(range.end_column - range.column, 5);
            }
        }
    }
}

#[test]
fn shared_pattern_context_accepts_bare_java_calls() {
    let source = "class Demo { void run() { target(value); other(value); } }";
    assert_fragment_context("java", source, "target($VALUE)", "target(value)");
    assert!(run_pattern(source, "java", "absent($VALUE)").is_empty());
    assert_eq!(
        run_pattern(source, "java", "class $NAME { $$$BODY }").len(),
        1
    );
}

#[test]
fn shared_pattern_context_accepts_bare_calls_in_c_family_rust_and_go() {
    let cases = [
        ("rs", "fn run() { target(value); other(value); }"),
        ("c", "void run(void) { target(value); other(value); }"),
        ("h", "void run(void) { target(value); other(value); }"),
        ("cpp", "void run() { target(value); other(value); }"),
        (
            "go",
            "package main\nfunc run() { target(value); other(value) }",
        ),
    ];
    for (ext, source) in cases {
        assert_fragment_context(ext, source, "target($VALUE)", "target(value)");
        assert!(
            run_pattern(source, ext, "absent($VALUE)").is_empty(),
            "{ext}"
        );
    }
}

/// A Rust expression fragment that only parses as a statement takes the
/// `;` terminator whatever its kind, not only a call: `$A?`, `$X.await`,
/// `$A as $T` and `&mut $A` compile bare and match exactly what their
/// `;`-terminated spelling matches.
#[test]
fn rust_expression_fragments_take_the_statement_terminator() {
    let source = "async fn run(v: Vec<u8>) -> Option<u8> {\n    let a = parse(v)?;\n    let b = fut.await;\n    let c = n as u64;\n    take(&mut buf);\n    None\n}\n";
    for (pattern, expected) in [
        ("$A?", "parse(v)?"),
        ("$X.await", "fut.await"),
        ("$A as $T", "n as u64"),
        ("&mut $A", "&mut buf"),
    ] {
        let bare = run_pattern(source, "rs", pattern);
        let terminated = run_pattern(source, "rs", &format!("{pattern};"));
        assert_eq!(bare.len(), 1, "{pattern}");
        assert_eq!(bare[0].text, expected, "{pattern}");
        assert_eq!(
            bare.iter().map(|m| &m.text).collect::<Vec<_>>(),
            terminated.iter().map(|m| &m.text).collect::<Vec<_>>(),
            "{pattern}"
        );
    }
    // A bare metavariable compiles too: `$A;` unwraps to `$A`.
    assert_eq!(
        run_pattern(source, "rs", "$A").len(),
        run_pattern(source, "rs", "$A;").len()
    );
}

// Some grammars parse a bare `$$$BODY` expando identifier at statement
// position ambiguously: an unrecognized identifier looks like the start of a
// declaration, and tree-sitter's error recovery inserts a zero-width MISSING
// `;` node as its sibling. The compiled pattern's root can still be a
// legitimate, non-`is_error()` node (so `CompiledPattern::new` accepts it) —
// but without filtering, that spurious MISSING sibling could never match any
// real candidate child, silently breaking every `{ $$$BODY }`-shaped pattern
// (0 matches, no error). One test per affected grammar, split so one
// grammar's regression doesn't hide another's.

#[test]
fn multi_capture_body_matches_in_c_despite_missing_sibling() {
    let matches = run_pattern(
        "int foo(int x) {\n  return x;\n}\n",
        "c",
        "int $NAME($$$ARGS) { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "C matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["foo".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn multi_capture_body_matches_in_csharp_despite_missing_sibling() {
    // C# has no top-level member syntax at all — `public int Foo(...) {...}`
    // parsed standalone doesn't just leave a MISSING sibling (the C/C++
    // shape); the whole body ends up wrapped in an ERROR node, because
    // `public` isn't a valid modifier outside a class/struct/interface body.
    // `preprocess_pattern` wraps every C# pattern in a synthetic
    // `class __OctoWrap { ... }` (see `AgLanguage::class_wrap`) so the parser
    // has real member context, and `effective_pattern_root` unwraps through
    // that specific synthetic class by name (see `WRAP_MARKER`) to
    // reach the real member. `meta_from_node` is purely text-based, so the
    // leftover ERROR wrapper around `$$$BODY` doesn't block recognizing it
    // as a multi-capture once the root kind is right.
    let matches = run_pattern(
        "class Box {\n  public int Foo(int x) {\n    return x;\n  }\n}\n",
        "cs",
        "public int $NAME($$$ARGS) { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "C# matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["Foo".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn class_shaped_pattern_still_matches_in_csharp_despite_synthetic_wrap() {
    // The synthetic wrapper class must unwrap ONLY itself (matched by its
    // exact literal name, `__OctoWrap`) — a genuine `class $NAME { ... }`
    // pattern, once wrapped as `class __OctoWrap { class µNAME { ... } }`,
    // must still resolve its effective root to the INNER class, not get
    // stuck comparing the synthetic outer class's own (non-metavar) name
    // against real candidates.
    let matches = run_pattern(
        "class Box {\n  public int Foo(int x) {\n    return x;\n  }\n}\n",
        "cs",
        "class $NAME { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "C# matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["Box".to_string()][..])
    );
}

#[test]
fn csharp_statement_and_expression_patterns_match_inside_methods() {
    let source = "class W {\n  int Area() { return Helper(LIMIT); }\n  static int Run() { var s = \"😀\"; Log(s); return Helper(s.Length); }\n}\n";
    let calls = run_pattern(source, "cs", "Helper($A)");
    assert_eq!(calls.len(), 2, "expression pattern");
    assert_eq!(calls[1].metavars["A"], ["s.Length"]);
    assert_eq!(
        run_pattern(source, "cs", "Log($A);").len(),
        1,
        "statement pattern"
    );
    assert_eq!(
        run_pattern(source, "cs", "Helper(LIMIT)").len(),
        1,
        "literal pattern"
    );
}

#[test]
fn rust_raw_identifier_borrow_parses_without_recovery_and_matches() {
    let source = "fn main() { let raw = 1; inspect(&raw); }";
    let language = lang("rs").tree_sitter_language();
    let tree = parse_tree(&language, source).expect("valid Rust parses");
    assert!(
        !tree.root_node().has_error(),
        "borrowing an identifier named raw is valid Rust: {}",
        tree.root_node().to_sexp()
    );
    let matches = run_pattern(source, "rs", "inspect($X)");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["X"], vec!["&raw"]);
}

#[test]
fn jsx_tag_pattern_matches_elements_only_in_jsx_grammars() {
    let source = "const view = <div className=\"x\"><Item id={1} /></div>;\n";
    for ext in ["tsx", "jsx", "js"] {
        let matches = run_pattern(source, ext, "<$T>");
        let tags = matches
            .iter()
            .map(|m| m.metavars["T"][0].as_str())
            .collect::<Vec<_>>();
        assert_eq!(tags, ["div", "Item"], "{ext}");
        assert_eq!(matches[0].text, "<div className=\"x\">", "{ext}");
        assert_eq!(matches[1].text, "<Item id={1} />", "{ext}");
    }
    // Rules share the same special matcher and backreference semantics.
    assert_eq!(run_rule(source, "tsx", json!({"pattern":"<$T>"})).len(), 2);

    // Grammars without JSX compile `<$T>` as an ordinary pattern: it never
    // takes the tag path (and so never matches JSX-looking text).
    for ext in ["ts", "py", "rs"] {
        if let Ok(matcher) = compile_matcher(
            &lang(ext),
            &StructuralQuery::new(Some("<$T>"), None).expect("query"),
        ) {
            assert!(
                all_matches(&matcher, "let x = 1;\n")
                    .map(|m| m.is_empty())
                    .unwrap_or(true),
                "{ext}"
            );
        }
    }
}

#[test]
fn key_value_pattern_is_gated_on_grammars_with_pairs() {
    let matches = run_pattern("const o = { a: 1, b: two };\n", "ts", "$K: $V");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[1].metavars["K"], ["b"]);
    assert_eq!(matches[1].metavars["V"], ["two"]);
    let py = run_pattern("d = {'a': 1}\n", "py", "$K: $V");
    assert_eq!(py.len(), 1);
    assert_eq!(py[0].metavars["K"], ["'a'"]);
}
