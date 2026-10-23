//! Query parsing and error codes, in single-content and directory searches.

use super::*;

#[test]
fn public_structural_errors_retain_native_codes() {
    for (pattern, rule, expected) in [
        (Some("fn ???"), None, "structural.query.compileFailed"),
        (
            None,
            Some(json!({"kind":"not_a_real_kind"})),
            "structural.query.compileFailed",
        ),
        (
            None,
            Some(json!({"kind":["function_item"]})),
            "structural.rule.invalid",
        ),
        (
            None,
            Some(json!({"kindx":"call"})),
            "structural.rule.invalid",
        ),
        (None, None, "structural.query.invalid"),
    ] {
        let error = search("fn main() {}", "rs", pattern, rule.as_ref())
            .err()
            .expect("invalid query");
        assert!(error.starts_with(&format!("[{expected}] ")), "{error}");
    }
    let unsupported = search("echo hi", "sh", Some("$$$"), None)
        .err()
        .expect("unsupported language");
    assert!(unsupported.starts_with("[structural.language.unsupported] "));

    let root = TmpDir::new();
    fs::write(root.join("fixture.rs"), "fn main() {}").unwrap();
    let result = search_files_detailed(review_file_options(
        &root,
        json!({"kind":"not_a_real_kind"}),
        10,
    ))
    .expect("typed failure");
    assert_eq!(result.status, "parserFailed");
    assert!(diagnostic_codes(&result).contains(&"structural.query.compileFailed"));
}

/// A rule that does not parse as a rule (an unknown key, a wrong value type, no
/// matcher, a bad regex) is `structural.rule.invalid`, apart from a rule or
/// pattern this grammar cannot compile (`structural.query.compileFailed`),
/// in a single-content search and in a file scan alike.
#[test]
fn rule_parse_errors_have_their_own_code() {
    let root = TmpDir::new();
    fs::write(root.join("fixture.rs"), "fn main() {}").unwrap();
    for (rule, expected) in [
        (json!({"kind":["function_item"]}), "structural.rule.invalid"),
        (json!({"kindx":"function_item"}), "structural.rule.invalid"),
        (json!({"has":{"field":"name"}}), "structural.rule.invalid"),
        (json!({"stopBy":"end"}), "structural.rule.invalid"),
        (json!({"regex":"("}), "structural.rule.invalid"),
        (
            json!({"kind":"not_a_real_kind"}),
            "structural.query.compileFailed",
        ),
        (
            json!({"pattern":"fn ???"}),
            "structural.query.compileFailed",
        ),
    ] {
        let single = evaluate_source("fn main() {}", "fixture.rs", "rs", None, Some(&rule));
        assert_eq!(single.status, "parserFailed", "{rule}");
        assert_eq!(single.diagnostics.len(), 1, "{rule}");
        assert_eq!(single.diagnostics[0].code, expected, "{rule}");
        let scan =
            search_files_detailed(review_file_options(&root, rule.clone(), 10)).expect("scan");
        assert_eq!(scan.status, "parserFailed", "{rule}");
        assert!(diagnostic_codes(&scan).contains(&expected), "{rule}");
    }
}

#[test]
fn directory_patterns_and_composed_rules_share_fragment_context() {
    let root = TmpDir::new();
    for (ext, source, pattern) in [
        (
            "java",
            "class Demo { void run() { target(value); } }",
            "target($X)",
        ),
        ("css", ".demo { color: value; }", "color: $VALUE"),
    ] {
        if languages::find_entry(ext).is_none() {
            continue;
        }
        fs::write(root.join(format!("fixture.{ext}")), source).unwrap();
        let rule = json!({"all": [{"pattern": pattern}, {"regex": "."}]});
        for use_rule in [true, false] {
            let mut options = review_file_options(&root, rule.clone(), 10);
            options.include = Some(vec![format!("*.{ext}")]);
            if !use_rule {
                options.rule = None;
                options.pattern = Some(pattern.to_owned());
            }
            let result = search_files_detailed(options).expect("fragment directory search");
            assert_eq!(result.status, "ok");
            assert_eq!(result.total_matches, 1, "{ext}: rule={use_rule}");
        }
    }
}

#[test]
fn ast_audit_unanchored_directory_reports_unsupported_files() {
    let root = TmpDir::new();
    fs::write(root.join("unsupported.sh"), "echo hello\n").expect("shell fixture");
    let mut options = review_file_options(&root, json!({"kind":"call_expression"}), 10);
    options.pattern = Some("$$$".to_owned());
    options.rule = None;
    options.include = Some(vec!["*.sh".to_owned()]);
    let result = search_files_detailed(options).expect("directory search");
    assert_eq!(result.status, "unsupported");
    assert_eq!(result.skipped_unsupported, 1);
    assert_eq!(result.total_matches, 0);
    let query = &result.query;
    assert_eq!(query.kind, "pattern");
    assert_eq!(query.pre_filter, "disabled");
    assert!(diagnostic_codes(&result).contains(&"structural.language.unsupported"));
}

#[test]
fn structural_review_deep_document_keeps_recall() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let result = evaluate_source(&source, "deep.ts", "ts", Some("probe()"), None);
    assert_eq!(result.status, "ok");
    assert_eq!(result.matches.len(), 1);
}

#[test]
fn structural_review_rule_prefilter_preserves_directory_recall() {
    let root = TmpDir::new();
    let source = "function present() { return 1; }";
    fs::write(root.join("fixture.ts"), source).expect("fixture");
    for rule in [
        json!({"kind":"function_declaration","not":{"pattern":"absent_probe($X)"}}),
        json!({"any":[{"kind":"function_declaration"},{"pattern":"absent_probe($X)"}]}),
        json!({"any":[{"pattern":"$X"},{"pattern":"absent_probe($X)"}]}),
        json!({"all":[{"kind":"function_declaration"},{"not":{"pattern":"absent_probe($X)"}}]}),
    ] {
        let direct = search(source, "ts", None, Some(&rule)).expect("direct search");
        let directory = search_files_detailed(StructuralSearchFilesOptions {
            path: root.to_string_lossy().into_owned(),
            pattern: None,
            rule: Some(rule.clone()),
            include: None,
            exclude: None,
            exclude_dir: None,
            default_exclude_dir: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: None,
            skip_files: None,
            max_file_bytes: None,
        })
        .expect("directory search");
        assert!(!direct.is_empty(), "fixture must match {rule}");
        assert_eq!(directory.total_matches as usize, direct.len(), "{rule}");
    }
}

#[test]
fn both_or_neither_query_errors() {
    assert!(search("x", "ts", Some("a"), Some(&json!({"pattern": "b"}))).is_err());
    assert!(search("x", "ts", None, None).is_err());
}

#[test]
fn invalid_pattern_errors() {
    assert!(search("x", "ts", Some("   "), None).is_err());
}

/// A pattern the grammar cannot parse is an error, not an empty result:
/// tree-sitter recovers `foo(` with a MISSING `)` the pattern never meant.
#[test]
fn unterminated_patterns_are_invalid_not_empty() {
    for (ext, pattern) in [
        ("ts", "foo("),
        ("ts", "if ($A"),
        ("py", "def f("),
        ("rs", "foo("),
    ] {
        let error = search("foo(1);\n", ext, Some(pattern), None)
            .err()
            .unwrap_or_else(|| panic!("{ext} {pattern} must be rejected"));
        assert!(error.contains("syntax errors"), "{ext} {pattern}: {error}");
    }
    // Statement terminators a pattern leaves off stay valid.
    for (ext, pattern) in [
        ("ts", "foo($A)"),
        ("c", "foo($A)"),
        ("go", "foo($A)"),
        ("cs", "Log($A)"),
    ] {
        assert!(
            search("x", ext, Some(pattern), None).is_ok(),
            "{ext} {pattern}"
        );
    }
}
