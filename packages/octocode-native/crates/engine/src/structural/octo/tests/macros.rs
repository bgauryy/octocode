//! Matching inside Rust macro token trees and C-family #define bodies.

use super::*;

const RUST_MACRO_SOURCE: &str = "fn t() {\n    assert!(status.unwrap().success());\n    assert_eq!(&read.unwrap(), b\"x\");\n    let v = vec![a.unwrap(); 2];\n    outer!(inner!(deep.unwrap()));\n    plain.unwrap();\n}\n";

/// A match inside a macro body spells only its own node's literals: an
/// `inside` literal (the macro's name) sits outside the body, so it cannot
/// decide whether the body is re-parsed. The anchored rule finds what the
/// same rule spelled with no literal (a non-literal regex) finds.
#[test]
fn a_macro_body_is_reparsed_when_only_an_enclosing_literal_names_the_macro() {
    let source = "fn t() {\n    assert!(check(x));\n    check(y);\n}\n";
    let texts = |rule: Value| -> Vec<String> {
        run_rule(source, "rs", rule)
            .into_iter()
            .map(|found| found.text)
            .collect()
    };
    let unanchored = texts(json!({
        "kind": "call_expression",
        "inside": {"kind": "macro_invocation", "has": {"kind": "identifier", "regex": "^asser[t]$"}, "stopBy": "end"}
    }));
    assert_eq!(unanchored, ["check(x)"]);
    let anchored = texts(json!({
        "kind": "call_expression",
        "inside": {"pattern": "assert!($$$)", "stopBy": "end"}
    }));
    assert_eq!(anchored, unanchored);
}

#[test]
fn rust_patterns_match_inside_macro_token_trees() {
    let found = run_pattern(RUST_MACRO_SOURCE, "rs", "$X.unwrap()");
    let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "status.unwrap()",
            "read.unwrap()",
            "a.unwrap()",
            "deep.unwrap()",
            "plain.unwrap()"
        ]
    );
    // Positions stay in file coordinates.
    assert_eq!((found[0].start_line, found[0].start_col), (2, 12));
    assert_eq!(found[0].metavar_ranges["X"][0].line, 2);
    let rule = run_rule(RUST_MACRO_SOURCE, "rs", json!({"pattern":"$X.unwrap()"}));
    assert_eq!(rule.len(), 5);
}

/// A DSL macro body re-parsed as code never matches inside its strings:
/// `json!({"pattern":"pub fn source() {}"})` holds no function.
#[test]
fn rust_macro_bodies_never_match_inside_string_literals() {
    let source = "fn t() {\n    let q = json!({\n        \"operation\":\"match\",\"path\":root,\n        \"pattern\":\"pub fn source() {}\",\"maxFiles\":1\n    });\n}\n";
    let found = run_pattern(source, "rs", "fn $NAME() {}");
    assert!(
        found
            .iter()
            .all(|m| m.text != "fn source() {}" && m.text != "pub fn source() {}"),
        "{:?}",
        found.iter().map(|m| &m.text).collect::<Vec<_>>()
    );
}

#[test]
fn rust_macro_bodies_keep_enclosing_ancestors_and_skip_the_delimiter_wrapper() {
    let inside = run_rule(
        RUST_MACRO_SOURCE,
        "rs",
        json!({"pattern":"$X.unwrap()","inside":{"kind":"function_item","stopBy":"end"}}),
    );
    assert_eq!(inside.len(), 5);
    let not_inside = run_rule(
        RUST_MACRO_SOURCE,
        "rs",
        json!({"pattern":"$X.unwrap()","not":{"inside":{"kind":"function_item","stopBy":"end"}}}),
    );
    assert!(not_inside.is_empty());
    // `assert!(x)`'s own parentheses are delimiters, not a parenthesized expression.
    let tuples = run_rule(
        "fn t() { assert_eq!(a, b); f((c, d)); }\n",
        "rs",
        json!({"kind":"tuple_expression"}),
    );
    assert_eq!(tuples.len(), 1);
    assert_eq!(tuples[0].text, "(c, d)");
    // The kind-only fast path expands macro bodies too.
    assert_eq!(
        run_rule(RUST_MACRO_SOURCE, "rs", json!({"kind":"call_expression"})).len(),
        6
    );
}

/// A `macro_rules!` template body is searched like an invocation's
/// arguments (re-parsed in place), never silently skipped.
#[test]
fn rust_macro_rules_template_bodies_are_searched() {
    let source = "macro_rules! get {\n    ($e:expr) => {\n        config.value.unwrap()\n    };\n}\nfn main() {\n    other.unwrap();\n}\n";
    let found = run_pattern(source, "rs", "$X.unwrap()");
    let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts, ["config.value.unwrap()", "other.unwrap()"]);
    assert_eq!((found[0].start_line, found[0].start_col), (3, 8));
    // The matcher pattern half (`($e:expr)`) is no source and stays flat.
    assert!(run_rule(source, "rs", json!({"kind":"parenthesized_expression"})).is_empty());
}

#[test]
fn rust_macro_bodies_without_the_anchor_keep_flat_tokens() {
    // Flat token-tree identifiers already match; a re-parsed body replaces
    // its flat tokens, so nothing is reported twice.
    let found = run_rule(
        "fn t() { assert!(ready); log!(ready, other(ready)); }\n",
        "rs",
        json!({"kind":"identifier","regex":"^ready$"}),
    );
    assert_eq!(found.len(), 3);
    // A body without the pattern's anchor keeps its flat tokens.
    assert!(run_pattern("fn t() { assert!(ok); }\n", "rs", "$X.unwrap()").is_empty());
}

const C_DEFINE_SOURCE: &str = "#define NODE_FREE(array) zfree((array)->alloc)\n#define LIMIT limit(8)\n#define RESET(p) do { p = zfree(p); } while (0)\n#define WRAPPED(p) \\\n    zfree(p)\nvoid f(void) { zfree(q); }\n";

/// `#define` bodies are opaque `preproc_arg` text to tree-sitter; they are
/// re-parsed in place in the file's own grammar (C, C++ and CUDA alike), so a
/// pattern reaches the calls inside them at file coordinates.
#[test]
fn c_family_patterns_match_inside_define_bodies() {
    let assert_matches = |ext: &str| {
        let found = run_pattern(C_DEFINE_SOURCE, ext, "zfree($$$A)");
        let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            texts,
            ["zfree((array)->alloc)", "zfree(p)", "zfree(p)", "zfree(q)"],
            ".{ext}"
        );
        assert_eq!((found[0].start_line, found[0].start_col), (1, 25), ".{ext}");
        assert_eq!((found[1].start_line, found[1].start_col), (3, 26), ".{ext}");
        // A body continued past a backslash-newline keeps its own line.
        assert_eq!((found[2].start_line, found[2].start_col), (5, 4), ".{ext}");
        assert_eq!(found[0].metavar_ranges["A"][0].line, 1, ".{ext}");
        let limit = run_pattern(C_DEFINE_SOURCE, ext, "limit($N)");
        assert_eq!(limit.len(), 1, ".{ext}");
        assert_eq!(limit[0].metavars["N"], ["8"], ".{ext}");
    };
    assert_matches("c");
    #[cfg(feature = "tree-sitter-cpp")]
    assert_matches("cpp");
    #[cfg(feature = "tree-sitter-cuda")]
    assert_matches("cu");
}

#[test]
fn c_family_define_bodies_keep_enclosing_ancestors_and_the_kind_fast_path() {
    let inside = run_rule(
        C_DEFINE_SOURCE,
        "c",
        json!({"pattern":"zfree($$$A)","inside":{"kind":"preproc_function_def","stopBy":"end"}}),
    );
    assert_eq!(inside.len(), 3);
    let outside = run_rule(
        C_DEFINE_SOURCE,
        "c",
        json!({"pattern":"zfree($$$A)","not":{"inside":{"kind":"preproc_function_def","stopBy":"end"}}}),
    );
    assert_eq!(outside.len(), 1);
    // The whole body can be the match: no delimiter wrapper hides it.
    assert_eq!(
        run_rule(C_DEFINE_SOURCE, "c", json!({"kind":"call_expression"})).len(),
        5
    );
}

/// A body that is no expression or statement (a bare brace, a type, a token
/// paste, a string, nothing) yields no match and no recovered syntax.
#[test]
fn c_family_define_bodies_that_are_not_code_add_no_noise() {
    let source = "#define OPEN {\n#define TYPE unsigned int\n#define EMPTY\n#define CAT(a, b) a ## b\n#define MSG \"zfree(x)\"\n#define LIST 1, 2, 3\n";
    let assert_no_noise = |ext: &str| {
        assert!(run_pattern(source, ext, "zfree($$$A)").is_empty(), ".{ext}");
        assert!(
            run_rule(source, ext, json!({"kind":"ERROR"})).is_empty(),
            ".{ext}"
        );
        assert!(
            run_rule(source, ext, json!({"kind":"call_expression"})).is_empty(),
            ".{ext}"
        );
    };
    assert_no_noise("c");
    #[cfg(feature = "tree-sitter-cpp")]
    assert_no_noise("cpp");
    #[cfg(feature = "tree-sitter-cuda")]
    assert_no_noise("cu");
}

#[test]
fn macro_source_units_keep_outer_captures_in_original_representation() {
    use crate::text::analysis::*;
    let text = "fn outer() {\n assert!(value.unwrap());\n}\n";
    let rule = json!({"pattern":"$X.unwrap()","inside":{"pattern":"fn $NAME() { $$$BODY }","stopBy":"end"}});
    let found = run_rule(text, "rs", rule);
    assert_eq!(found.len(), 1);
    let matched = &found[0];
    assert_ne!(matched.source_span.unit, UnitHandle(0));
    let name = &matched.metavar_ranges["NAME"][0];
    assert_eq!(name.source_span.unit, UnitHandle(0));
    assert_eq!(
        &text[name.source_span.start_byte as usize..name.source_span.end_byte as usize],
        "outer"
    );
    let mut budget = MapBudget::new(
        MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    let admitted = AdmittedSource::from_bytes(text.as_bytes(), "a.rs", &mut budget).unwrap();
    let bound = matched
        .source_templates
        .bind(&admitted, &mut budget)
        .unwrap();
    let main = bound.project(matched.source_span, &mut budget).unwrap();
    let outer = bound.project(name.source_span, &mut budget).unwrap();
    assert_ne!(main.analysis_unit_id, outer.analysis_unit_id);
    assert_ne!(
        bound
            .representation_for_unit(matched.source_span.unit)
            .unwrap()
            .id,
        bound
            .representation_for_unit(name.source_span.unit)
            .unwrap()
            .id
    );
    assert!(
        main.segments
            .iter()
            .all(|s| s.analysis == AnalysisMembership::Included)
    );
}

#[test]
fn synthetic_c_tail_is_unmapped_and_never_a_match() {
    use crate::text::analysis::*;
    let text = "#define RUN(x) work(x)\n";
    let found = run_pattern(text, "c", "work($X)");
    assert_eq!(found.len(), 1);
    let matched = &found[0];
    assert_eq!(matched.text, "work(x)");
    let mut budget = MapBudget::new(
        MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    let admitted = AdmittedSource::from_bytes(text.as_bytes(), "a.c", &mut budget).unwrap();
    let bound = matched
        .source_templates
        .bind(&admitted, &mut budget)
        .unwrap();
    let descriptor = bound
        .representation_for_unit(matched.source_span.unit)
        .unwrap();
    let tail = bound
        .project(
            ObservedSpan {
                unit: matched.source_span.unit,
                range: ByteRange::new(descriptor.byte_length - 1, descriptor.byte_length),
            },
            &mut budget,
        )
        .unwrap();
    assert_eq!(
        tail.segments[0].branches,
        vec![OriginBranch::Unmapped(UnmappedReason::Synthetic)]
    );
    let occurrence = bound.project(matched.source_span, &mut budget).unwrap();
    assert!(
        occurrence
            .segments
            .iter()
            .flat_map(|s| &s.branches)
            .all(|b| matches!(b, OriginBranch::Copied(_)))
    );
}

#[test]
fn macro_projection_budget_profile_uses_real_producer() {
    use crate::text::analysis::*;
    let mut text = "fn run() {\n".to_owned();
    for index in 0..256 {
        text.push_str(&format!("assert!(value{index}.unwrap());\n"));
    }
    text.push_str("}\n");
    let mut matches = run_pattern(&text, "rs", "$X.unwrap()");
    assert_eq!(matches.len(), 256);
    let mut budget = MapBudget::new(
        MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        &|| false,
    );
    let admitted = AdmittedSource::from_bytes(text.as_bytes(), "macros.rs", &mut budget).unwrap();
    let mut binder = crate::structural::SourceBinder::with_budget(&admitted, budget);
    let mut origins = 0;
    for matched in &mut matches {
        binder.bind_raw_match(matched).unwrap();
        assert_ne!(matched.source_span.unit, UnitHandle(0));
        let origin = matched.source_origin.as_ref().unwrap();
        origins += origin.owned_bytes();
        assert!(
            origin
                .segments
                .iter()
                .flat_map(|segment| &segment.branches)
                .all(|branch| matches!(branch, OriginBranch::Copied(_)))
        );
    }
    eprintln!(
        "256 real Rust macro matches: raw={} originOwned={origins} usage={:?}",
        text.len(),
        binder.usage()
    );
    assert_eq!(binder.usage().representations, 512);
    assert!(binder.usage().bytes < MapLimits::default().bytes);
}

/// The texts of a rule's matches, in source order.
fn rule_texts(source: &str, ext: &str, rule: Value) -> Vec<String> {
    run_rule(source, ext, rule)
        .into_iter()
        .map(|found| found.text)
        .collect()
}

/// `has` reads the tree `inside` reads: a call inside `assert!(…)` is a
/// descendant of the function around it, and its captures keep file
/// coordinates.
#[test]
fn has_through_every_level_reaches_into_rust_macro_bodies() {
    let source = "fn k() {\n    assert!(check(z));\n}\nfn j() {\n    check(z);\n}\n";
    let found = run_rule(
        source,
        "rs",
        json!({"kind": "function_item", "has": {"pattern": "check($A)", "stopBy": "end"}}),
    );
    let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "fn k() {\n    assert!(check(z));\n}",
            "fn j() {\n    check(z);\n}"
        ]
    );
    assert_eq!(found[0].metavars["A"], ["z"]);
    let capture = &found[0].metavar_ranges["A"][0];
    assert_eq!((capture.line, capture.column), (2, 18));
    assert_eq!(
        &source[capture.source_span.start_byte as usize..capture.source_span.end_byte as usize],
        "z"
    );
    // The inverse relation finds the same call.
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({"pattern": "check($A)", "inside": {"kind": "function_item", "stopBy": "end"}}),
        ),
        ["check(z)", "check(z)"]
    );
}

/// A `has` target that only a kind names still finds a call inside a body,
/// with no literal anywhere and with the literal only on the outer node.
#[test]
fn has_by_kind_alone_reaches_into_a_macro_body() {
    let source = "fn f() {\n    outer_fn(m!(zzz()));\n    outer_fn(1);\n}\n";
    for rule in [
        json!({"kind": "call_expression", "has": {"kind": "call_expression", "stopBy": "end"}}),
        json!({"pattern": "outer_fn($$$)", "has": {"kind": "call_expression", "stopBy": "end"}}),
    ] {
        assert_eq!(
            rule_texts(source, "rs", rule.clone()),
            ["outer_fn(m!(zzz()))"],
            "{rule}"
        );
    }
}

/// A neighbor-only `has` sees a body's top nodes as the children of the
/// token tree that holds them, as `inside` sees that token tree as their
/// parent.
#[test]
fn neighbor_has_and_inside_agree_across_a_rust_macro_body() {
    let source = "fn f() {\n    assert!(check(z));\n}\n";
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "token_tree", "has": {"pattern": "check($A)"}})
        ),
        ["(check(z))"]
    );
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({"pattern": "check($A)", "inside": {"kind": "token_tree"}})
        ),
        ["check(z)"]
    );
    // The call is a grandchild of the invocation, not a child.
    assert!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "macro_invocation", "has": {"pattern": "check($A)"}})
        )
        .is_empty()
    );
}

/// The same for a C `#define` body, the value of its `preproc_arg`.
#[test]
fn has_reaches_into_c_define_bodies() {
    let source = "#define CALL(x) foo(x)\n#define NONE(x) bar(x)\nvoid g(void) { foo(1); }\n";
    let found = run_rule(
        source,
        "c",
        json!({"kind": "preproc_function_def", "has": {"pattern": "foo($A)", "stopBy": "end"}}),
    );
    let texts: Vec<_> = found.iter().map(|m| m.text.trim_end()).collect();
    assert_eq!(texts, ["#define CALL(x) foo(x)"]);
    assert_eq!(found[0].metavars["A"], ["x"]);
    assert_eq!(found[0].metavar_ranges["A"][0].line, 1);
    let neighbor = run_rule(
        source,
        "c",
        json!({"kind": "preproc_arg", "has": {"pattern": "foo($A)"}}),
    );
    assert_eq!(neighbor.len(), 1);
    assert_eq!(neighbor[0].metavars["A"], ["x"]);
    assert_eq!(
        rule_texts(
            source,
            "c",
            json!({"pattern": "foo($A)", "inside": {"kind": "preproc_arg"}})
        ),
        ["foo(x)"]
    );
}

/// `precedes` and `follows` test their siblings with the same tree: a
/// sibling statement whose call sits inside a macro body.
#[test]
fn precedes_and_follows_see_has_targets_inside_macro_bodies() {
    let source = "fn f() {\n    start();\n    assert!(check(z));\n    stop();\n}\n";
    let checked =
        json!({"kind": "expression_statement", "has": {"pattern": "check($A)", "stopBy": "end"}});
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "expression_statement", "precedes": checked.clone()})
        ),
        ["start();"]
    );
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "expression_statement", "follows": checked})
        ),
        ["stop();"]
    );
    // Siblings inside one body.
    let body = "#define F(a) do { first(a); check(a); } while (0)\n";
    assert_eq!(
        rule_texts(
            body,
            "c",
            json!({
                "kind": "expression_statement",
                "has": {"pattern": "first($A)"},
                "precedes": {"kind": "expression_statement", "has": {"pattern": "check($A)"}}
            })
        ),
        ["first(a);"]
    );
}

/// A negated `has` sees the body too: the macro-body gate re-parses a body
/// holding the `has` target's literal, though the matched node's own
/// literal sits outside it.
#[test]
fn a_negated_has_reads_macro_bodies_the_outer_literal_does_not_reach() {
    let source = "fn k() { assert!(check(z)); }\nfn kk() { ok(); }\n";
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({
                "kind": "function_item",
                "regex": "^fn k",
                "not": {"has": {"pattern": "check($A)", "stopBy": "end"}}
            })
        ),
        ["fn kk() { ok(); }"]
    );
}

/// RV-C3eH probe: the fallback chain (`ExpandedTree::ancestors`, for a
/// caller with no walk chain) equals the chain the walk hands every node,
/// body nodes included. Sources: the fixtures below, plus every `.rs`/`.c`/
/// `.h` file under `RVC3EH_CHAIN_ROOT` when set (`-- --ignored`).
fn rv_chain_mismatches(ext: &str, source: &str) -> (usize, usize, Vec<String>) {
    use crate::structural::octo::expanded::ExpandedTree;
    use crate::structural::octo::macro_bodies::MacroBodies;
    use crate::structural::octo::matching::Deadline;
    let lang = lang(ext);
    let language = lang.tree_sitter_language();
    let tree = parse_tree(&language, source).expect("parses");
    let macros = MacroBodies::for_language(&lang, None);
    let expanded = ExpandedTree::new(
        tree.root_node(),
        source,
        &language,
        macros.as_ref(),
        Deadline::UNBOUNDED,
    )
    .expect("no deadline");
    let mut visited = Vec::new();
    expanded
        .visit_nodes(Deadline::UNBOUNDED, &mut |node, chain, _| {
            visited.push((
                node,
                chain.iter().map(tree_sitter::Node::id).collect::<Vec<_>>(),
                chain
                    .iter()
                    .map(|n| format!("{}@{}..{}", n.kind(), n.start_byte(), n.end_byte()))
                    .collect::<Vec<_>>()
                    .join(" > "),
            ));
            Ok(())
        })
        .expect("no deadline");
    let mut body_nodes = 0;
    let mut bad = Vec::new();
    for (node, chain, kinds) in &visited {
        let fallback_nodes = expanded
            .ancestors(*node, Deadline::UNBOUNDED)
            .expect("no deadline");
        let fallback: Vec<usize> = fallback_nodes.iter().map(tree_sitter::Node::id).collect();
        let fallback_kinds = fallback_nodes
            .iter()
            .map(|n| format!("{}@{}..{}", n.kind(), n.start_byte(), n.end_byte()))
            .collect::<Vec<_>>()
            .join(" > ");
        if crate::structural::octo::source_context::tree_id(*node)
            != crate::structural::octo::source_context::tree_id(tree.root_node())
        {
            body_nodes += 1;
        }
        if &fallback != chain && bad.len() < 5 {
            bad.push(format!(
                "{} {}..{} {:?}: walk {} ancestors, fallback {}\n  walk: {kinds}\n  fall: {fallback_kinds}",
                node.kind(),
                node.start_byte(),
                node.end_byte(),
                source[node.byte_range()].chars().take(40).collect::<String>(),
                chain.len(),
                fallback.len()
            ));
        }
    }
    (visited.len(), body_nodes, bad)
}

#[test]
fn rv_c3eh_fallback_chain_equals_the_walk_chain() {
    let sources = [
        ("rs", RUST_MACRO_SOURCE),
        (
            "rs",
            "macro_rules! wrap {\n    ($e:expr) => { check($e).unwrap() };\n}\nfn u() { wrap!(1); println!(\"{}\", helper(2)); let q = json!({\"k\": \"fn a() {}\", \"v\": f(1)}); m!(n!(o!(p(1)))); }\n",
        ),
        (
            "rs",
            "fn t(){ assert!(check(x)); assert_eq!(a(), b()); vec![a.unwrap(); 2]; m!{ let z = y(1); }; }\n",
        ),
        (
            "rs",
            "fn f() { let v = json!({\"dir\": format!(\"bundled:{name}\"), \"md\": skill.text()}); }\n",
        ),
        (
            "c",
            "#define CHECK(x) check(x)\n#define TWO(p) a(p); b(p)\n#define BLOCK do { zfree(p); } while (0)\nint helper(int n) { return CHECK(n) + zfree(0); }\n",
        ),
    ];
    let mut body_total = 0;
    for (ext, source) in sources {
        let (visited, body, bad) = rv_chain_mismatches(ext, source);
        body_total += body;
        assert!(
            bad.is_empty(),
            "{ext}: {visited} nodes, {body} in bodies: {bad:#?}"
        );
    }
    assert!(body_total > 0, "the fixtures reached no body node");
}

#[test]
#[ignore = "reviewer run on demand: RVC3EH_CHAIN_ROOT=<dir>"]
fn rv_c3eh_fallback_chain_over_a_tree() {
    let root =
        std::path::PathBuf::from(std::env::var("RVC3EH_CHAIN_ROOT").expect("RVC3EH_CHAIN_ROOT"));
    let mut stack = vec![root];
    let (mut files, mut nodes, mut body, mut failures) = (0, 0, 0, Vec::new());
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !path.ends_with("target") && !path.ends_with(".git") {
                    stack.push(path);
                }
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "rs" | "c" | "h") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (v, b, bad) = rv_chain_mismatches(ext, &source);
            files += 1;
            nodes += v;
            body += b;
            if !bad.is_empty() && failures.len() < 10 {
                failures.push(format!("{}: {bad:?}", path.display()));
            }
        }
    }
    eprintln!(
        "rv chain: {files} files, {nodes} nodes, {body} body nodes, {} files mismatched",
        failures.len()
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// RV-C3eH probe: a string literal is a real sibling inside a macro body;
/// `Some(2)` is the second argument, so `nthChild: 1` must not match it.
#[test]
fn rv_c3eh_nth_child_counts_a_string_sibling_inside_a_body() {
    let source = "fn t() {\n    assert_eq!(r.report(\"a\", Some(2), false), 1);\n    r.report(\"a\", Some(3), false);\n}\n";
    let texts = |rule: Value| -> Vec<String> {
        run_rule(source, "rs", rule)
            .into_iter()
            .map(|m| m.text)
            .collect()
    };
    assert_eq!(
        texts(json!({"pattern": "Some($A)", "nthChild": 2})),
        ["Some(2)", "Some(3)"]
    );
    assert_eq!(
        texts(json!({"pattern": "Some($A)", "nthChild": 1})),
        Vec::<String>::new()
    );
}

/// RV-C3eH probe: a call inside a function that a body spells with
/// recovered syntax (`proptest!` parameters `x in 0..3`) is still inside
/// that function, as before the change.
#[test]
fn rv_c3eh_inside_keeps_an_ancestor_with_recovered_syntax_in_a_body() {
    let source = "proptest::proptest! {\n    #[test]\n    fn p(x in 0..3) {\n        y.unwrap();\n    }\n}\n";
    let texts = |rule: Value| -> Vec<String> {
        run_rule(source, "rs", rule)
            .into_iter()
            .map(|m| m.text)
            .collect()
    };
    assert_eq!(texts(json!({"pattern": "$X.unwrap()"})), ["y.unwrap()"]);
    assert_eq!(
        texts(
            json!({"pattern": "$X.unwrap()", "inside": {"kind": "function_item", "stopBy": "end"}})
        ),
        ["y.unwrap()"]
    );
}

/// RV-C3eH probe: an expression that begins with a string literal inside a
/// macro body is code, not text inside the string.
#[test]
fn rv_c3eh_a_call_on_a_string_literal_inside_a_body_is_a_candidate() {
    let source =
        "fn t() {\n    assert_eq!(a, \"foo\".to_string());\n    let b = \"foo\".to_string();\n}\n";
    let texts: Vec<String> = run_pattern(source, "rs", "$X.to_string()")
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert_eq!(texts, ["\"foo\".to_string()", "\"foo\".to_string()"]);
}

/// Text inside a body's string literal is no code: no relation reaches it,
/// as no match is ever found in it. The function around `json!` holds no
/// function, though the body's re-parse can read a string's text as one.
#[test]
fn no_relation_reaches_text_inside_a_body_string() {
    let source = "fn t() {\n    let q = json!({\n        \"operation\":\"match\",\"path\":root,\n        \"pattern\":\"pub fn source() {}\",\"maxFiles\":1\n    });\n}\n";
    assert!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "function_item", "has": {"kind": "function_item", "stopBy": "end"}})
        )
        .is_empty()
    );
    assert!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "block", "inside": {"kind": "macro_invocation", "stopBy": "end"}})
        )
        .iter()
        .all(|text| !text.contains("fn source"))
    );
}

/// The first line of each match of `rule`, and the macro bodies whose
/// recovered re-parse could hide a match (`name line`).
fn rule_heads_and_bodies(source: &str, ext: &str, rule: &Value) -> (Vec<String>, Vec<String>) {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(None, Some(rule)).expect("query"),
    )
    .expect("compile rule");
    let (found, recovered) = matches_with_recovery(&matcher, source).expect("complete execution");
    let heads = found
        .into_iter()
        .map(|m| m.matched.text.lines().next().unwrap_or_default().to_owned())
        .collect();
    let bodies = recovered
        .bodies
        .iter()
        .map(|body| format!("{} {}", body.name, body.line))
        .collect();
    (heads, bodies)
}

/// B214 (the live `p3` repro): a call directly inside a `json!` object is a
/// match. The object is no Rust, so its re-parse recovers; each macro call
/// its flat tokens spell (`path ! group`) is re-parsed on its own, so the
/// call has a node with its own extent.
#[test]
fn a_call_directly_inside_a_json_object_body_is_a_match() {
    let source = "use serde_json::json;\npub fn build(name: &str) -> serde_json::Value {\n    let plain = format!(\"plain {}\", name);\n    json!({\n        \"greeting\": format!(\"hello {}\", name),\n        \"nested\": { \"inner\": format!(\"deep {}\", name) },\n        \"plain\": plain,\n    })\n}\n";
    let found = run_rule(source, "rs", json!({"pattern": "format!($$$A)"}));
    let lines: Vec<(u32, &str)> = found
        .iter()
        .map(|m| (m.start_line, m.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (3, "format!(\"plain {}\", name)"),
            (5, "format!(\"hello {}\", name)"),
            (6, "format!(\"deep {}\", name)"),
        ]
    );
    // The captures resolve in file coordinates.
    assert_eq!(found[1].start_col, 20);
    // Paths and nested calls: each call keeps its own extent.
    let source = "fn f() {\n    let v = json!({\"a\": serde_json::json!({\"b\": vec![format!(\"x\")]}), \"c\": ::std::format!(\"y\")});\n}\n";
    let (heads, _) = rule_heads_and_bodies(source, "rs", &json!({"kind": "macro_invocation"}));
    assert_eq!(
        heads,
        [
            "json!({\"a\": serde_json::json!({\"b\": vec![format!(\"x\")]}), \"c\": ::std::format!(\"y\")})",
            "serde_json::json!({\"b\": vec![format!(\"x\")]})",
            "vec![format!(\"x\")]",
            "format!(\"x\")",
            "::std::format!(\"y\")",
        ]
    );
}

/// RV-W2b (live probe): every call inside a `json!` object is a match, not
/// only a macro call. Recovery can read a key and its value as one call
/// (`"id".len()`) or a key's quotes as a string around code; each part of
/// the object the body's re-parse could not read is re-parsed on its own.
#[test]
fn every_call_inside_a_json_object_body_is_a_match() {
    let source = "fn f() {\n    let j = json!({\"b\": {\"c\": y.len(), \"d\": 1}});\n    let k = json!({\"b\": {\"c\": y.len(), \"d\": z.len()}});\n    let m = json!({\"id\": x.len()});\n    let n = json!({\"c\": y.len(), \"d\": 1});\n}\n";
    let found = run_rule(source, "rs", json!({"pattern": "$X.len()"}));
    let at: Vec<(u32, u32, &str)> = found
        .iter()
        .map(|m| (m.start_line, m.start_col, m.text.as_str()))
        .collect();
    assert_eq!(
        at,
        [
            (2, 30, "y.len()"),
            (3, 30, "y.len()"),
            (3, 44, "z.len()"),
            (4, 25, "x.len()"),
            (5, 24, "y.len()"),
        ]
    );
    // Every part was read: no body is named as one that could hide a match.
    let (_, bodies) = rule_heads_and_bodies(source, "rs", &json!({"pattern": "$X.len()"}));
    assert!(bodies.is_empty(), "{bodies:?}");
}

/// Body text no re-parse could read is never a silent gap: when it holds
/// the query's anchor, its macro is named; text without it names nothing.
#[test]
fn unread_body_text_with_the_anchor_names_its_macro() {
    let rule = json!({"pattern": "$X.len()"});
    let (heads, bodies) = rule_heads_and_bodies("fn f() {\n    m!(a b c len, d.len());\n}\n", "rs", &rule);
    assert_eq!(heads, ["d.len()"]);
    assert_eq!(bodies, ["m! 2"]);
    let (heads, bodies) = rule_heads_and_bodies("fn f() {\n    m!(a b c, d.len());\n}\n", "rs", &rule);
    assert_eq!(heads, ["d.len()"]);
    assert!(bodies.is_empty(), "{bodies:?}");
}

/// A macro call re-parsed on its own reads as one clean `macro_invocation`
/// over exactly its text, with or without the `;` a part is read with
/// ([`super::super::macro_bodies::PART_TAIL`]).
#[test]
fn a_lone_macro_call_reparses_as_one_invocation() {
    use super::super::macro_bodies::{MacroBodies, PART_TAIL};
    let content = "x(json!({\"k\": format!(\"a{}\", b)}))";
    let start = content.find("format").expect("call");
    let end = content.rfind(")}").expect("call end") + 1;
    let point = |column| tree_sitter::Point { row: 0, column };
    let range = tree_sitter::Range {
        start_byte: start,
        end_byte: end,
        start_point: point(start),
        end_point: point(end),
    };
    for tail in ["", PART_TAIL] {
        let tree = MacroBodies::parse(
            content,
            &lang("rs").tree_sitter_language(),
            range,
            tail,
            Deadline::UNBOUNDED,
        )
        .expect("parse")
        .expect("a tree");
        let root = tree.root_node();
        assert!(!root.has_error(), "tail {tail:?}: {}", root.to_sexp());
        let mut invocations = Vec::new();
        let mut cursor = root.walk();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if node.kind() == "macro_invocation" {
                invocations.push(&content[node.byte_range()]);
            }
            pending.extend(node.named_children(&mut cursor));
        }
        assert_eq!(invocations, ["format!(\"a{}\", b)"], "tail {tail:?}");
    }
}

/// RV-C3eH round 3 probe: no match returned from a re-parsed `json!` object
/// has a wrong extent (text the real tree has no node for).
#[test]
fn rv_c3eh_r3_json_object_bodies_return_no_wrong_extent() {
    let source = "fn f(v: &V, i: u32) {\n    let a = json!({\"name\":format!(\"f{i:03}.rs\"),\"path\":format!(\"f{i:03}.rs\"),\"type\":\"file\",\"size\":1});\n    let b = json!({\"kind\": \"token_tree\", \"has\": {\"pattern\": \"check($A)\"}});\n    let c = json!({\n        \"id\": v[\"id\"].to_string().trim_matches('\"'),\n        \"author\": a.unwrap_or(\"unknown\"),\n    });\n}\n";
    let mut wrong = Vec::new();
    for rule in [
        json!({"pattern": "format!($$$)"}),
        json!({"kind": "block"}),
        json!({"kind": "call_expression"}),
        json!({"kind": "macro_invocation"}),
    ] {
        let matcher = compile_matcher(
            &lang("rs"),
            &StructuralQuery::new(None, Some(&rule)).expect("query"),
        )
        .expect("compile");
        let (found, _) = matches_with_recovery(&matcher, source).expect("run");
        for text in found.into_iter().map(|m| m.matched.text) {
            let bad = text.starts_with('"')
                || text.matches('(').count() != text.matches(')').count()
                || text.matches('{').count() != text.matches('}').count();
            if (bad && !text.contains("'\"'")) || text.starts_with("\"id\"") {
                wrong.push(format!("{rule}: {text}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    // The two `format!` calls of `a` are found with their own extents.
    assert_eq!(
        rule_texts(source, "rs", json!({"pattern": "format!($$$)"})),
        ["format!(\"f{i:03}.rs\")", "format!(\"f{i:03}.rs\")"]
    );
}

/// A string literal inside a body keeps its own children: only text that
/// the body's re-parse did not read as that literal is out of the tree.
#[test]
fn a_string_literal_in_a_body_keeps_its_own_children() {
    let source = "fn t() {\n    assert_eq!(a, \"x\\n\");\n    let b = \"x\\n\";\n}\n";
    assert_eq!(
        rule_texts(
            source,
            "rs",
            json!({"kind": "string_literal", "has": {"kind": "escape_sequence"}})
        ),
        ["\"x\\n\"", "\"x\\n\""]
    );
    assert_eq!(
        rule_texts(source, "rs", json!({"kind": "escape_sequence"})),
        ["\\n", "\\n"]
    );
}

/// A body node whose own tokens parsed, with recovered syntax only inside a
/// bracket group strictly within it (`proptest!`'s `x in 0..3` parameter
/// list), is a match, as it would be in the file's own tree.
#[test]
fn a_body_node_with_recovered_syntax_inside_its_own_groups_is_a_match() {
    let source = "proptest::proptest! {\n    #[test]\n    fn p(x in 0..3) {\n        y.unwrap();\n    }\n}\nfn q() { z.unwrap(); }\n";
    for rule in [
        json!({"kind": "function_item"}),
        json!({"kind": "function_item", "has": {"pattern": "$X.unwrap()", "stopBy": "end"}}),
    ] {
        let (heads, _) = rule_heads_and_bodies(source, "rs", &rule);
        assert_eq!(
            heads,
            ["fn p(x in 0..3) {", "fn q() { z.unwrap(); }"],
            "{rule}"
        );
    }
}

/// A body node the re-parse could not read (an ERROR node, or recovered
/// syntax at its own level) is no match; a rule that matches one names the
/// macro body and its line, and a clean body or an unmatched rule names none.
#[test]
fn a_matched_unreadable_body_node_names_its_macro_and_line() {
    let rule = json!({"kind": "ERROR"});
    let (heads, bodies) = rule_heads_and_bodies("fn t() {\n    m!(a b c);\n}\n", "rs", &rule);
    assert!(heads.is_empty(), "{heads:?}");
    assert_eq!(bodies, ["m! 2"]);
    let (heads, bodies) = rule_heads_and_bodies("fn t() { m!(a, b); }\n", "rs", &rule);
    assert!(heads.is_empty() && bodies.is_empty(), "{heads:?} {bodies:?}");
    let (_, bodies) = rule_heads_and_bodies(
        "fn t() { m!(a b c); }\n",
        "rs",
        &json!({"kind": "call_expression"}),
    );
    assert!(bodies.is_empty(), "{bodies:?}");
    // A `json!` object's key-led misread (`"id": v[…].to_string()` read as
    // one call) is no match; the value's own re-parse holds the call, so
    // nothing is hidden and the object is not named.
    let source = "fn f() {\n    let c = json!({\n        \"id\": v[\"id\"].to_string(),\n    });\n}\n";
    let (heads, bodies) = rule_heads_and_bodies(
        source,
        "rs",
        &json!({"kind": "call_expression", "regex": "^\""}),
    );
    assert!(heads.is_empty() && bodies.is_empty(), "{heads:?} {bodies:?}");
    let (heads, bodies) =
        rule_heads_and_bodies(source, "rs", &json!({"kind": "call_expression"}));
    assert_eq!(heads, ["v[\"id\"].to_string()"]);
    assert!(bodies.is_empty(), "{bodies:?}");
}
