//! Matching inside Rust macro token trees and C-family #define bodies.

use super::*;

const RUST_MACRO_SOURCE: &str = "fn t() {\n    assert!(status.unwrap().success());\n    assert_eq!(&read.unwrap(), b\"x\");\n    let v = vec![a.unwrap(); 2];\n    outer!(inner!(deep.unwrap()));\n    plain.unwrap();\n}\n";

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
#[expect(
    clippy::print_stderr,
    reason = "explicit resource-profile test emits its measured accounting receipt"
)]
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
