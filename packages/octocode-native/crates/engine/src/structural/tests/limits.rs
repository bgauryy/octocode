//! Execution limits, file caps and time budgets, each with explicit completion.

use super::*;

#[test]
fn structural_review_pattern_depth_is_explicitly_incomplete() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let pattern = format!("{}probe(){}", "[".repeat(600), "]".repeat(600));
    let result = evaluate_source(&source, "deep.ts", "ts", Some(&pattern), None);
    assert_eq!(result.status, "truncated");
    assert!(result.matches.is_empty());
    assert_eq!(result.diagnostics[0].code, "structural.match.depthLimit");
}

#[test]
fn structural_review_backtracking_exhaustion_cannot_satisfy_negation() {
    let source = format!("probe({});", vec!["1"; 150].join(","));
    let rule = json!({"kind":"call_expression","not":{"pattern":"probe($$$A, $$$B, absent)"}});
    let result = evaluate_source(&source, "wide.ts", "ts", None, Some(&rule));
    assert_eq!(result.status, "truncated");
    assert!(result.matches.is_empty());
    assert_eq!(
        result.diagnostics[0].code,
        "structural.match.backtrackingLimit"
    );
    let error = search(&source, "ts", None, Some(&rule))
        .err()
        .expect("legacy must fail explicitly");
    assert!(error.starts_with("[structural.match.backtrackingLimit]"));
}

#[test]
fn structural_review_scan_cap_and_depth_have_explicit_completion() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("aaa_nested")).expect("nested");
    fs::write(root.join("aaa_nested/hidden.ts"), "probe();").expect("deep");
    fs::write(root.join("one.ts"), "probe();").expect("first");
    for rule in [
        json!({"kind":"call_expression"}),
        json!({"pattern":"probe()"}),
        json!({"any":[{"pattern":"probe()"},{"pattern":"other()"}]}),
    ] {
        let exact =
            search_files_detailed(review_file_options(&root, rule.clone(), 1)).expect("exact cap");
        assert_eq!(
            exact.total_matches, 1,
            "native depth1 includes only direct files: {rule}"
        );
        assert!(!exact.scan_truncated, "exact cap is complete: {rule}");
        fs::write(root.join("two.ts"), "probe();").expect("second");
        let limited = search_files_detailed(review_file_options(&root, rule.clone(), 1))
            .expect("overflow cap");
        assert!(limited.scan_truncated, "overflow is explicit: {rule}");
        assert_eq!(limited.total_matches, 1);
        let detailed = search_files_detailed(review_file_options(&root, rule.clone(), 1))
            .expect("detailed overflow");
        assert!(detailed.scan_truncated);
        fs::remove_file(root.join("two.ts")).expect("reset fixture");
    }
}

#[test]
fn structural_review_file_limits_retain_completed_files() {
    let root = TmpDir::new();
    fs::write(root.join("ok.ts"), "probe(1);").expect("complete file");
    fs::write(
        root.join("wide.ts"),
        format!("probe({});", vec!["1"; 150].join(",")),
    )
    .expect("limited file");
    let rule = json!({"kind":"call_expression","not":{"pattern":"probe($$$A, $$$B, absent)"}});
    let result = search_files_detailed(review_file_options(&root, rule.clone(), 10))
        .expect("partial file search");
    assert_eq!(result.status, "truncated");
    assert_eq!(result.total_matches, 1);
    assert!(matched_paths(&result)[0].ends_with("ok.ts"));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].code,
        "structural.match.backtrackingLimit"
    );
    assert!(
        result.diagnostics[0]
            .path
            .as_deref()
            .expect("path")
            .ends_with("wide.ts")
    );
    let detailed =
        search_files_detailed(review_file_options(&root, rule, 10)).expect("detailed file search");
    assert_eq!(detailed.status, "truncated");
    assert_eq!(detailed.total_matches, 1);
    assert!(detailed.files.iter().any(|file| file.status == "truncated"));
    assert_eq!(detailed.diagnostics.len(), 1);
}

#[test]
fn structural_review_deep_relational_traversal_keeps_recall() {
    let source = format!("const x = {}probe(){};", "[".repeat(600), "]".repeat(600));
    let has = json!({"kind":"lexical_declaration","has":{"pattern":"probe()","stopBy":"end"}});
    assert_eq!(
        search(&source, "ts", None, Some(&has))
            .expect("deep has")
            .len(),
        1
    );
    let inside =
        json!({"pattern":"probe()","inside":{"kind":"lexical_declaration","stopBy":"end"}});
    assert_eq!(
        search(&source, "ts", None, Some(&inside))
            .expect("deep inside")
            .len(),
        1
    );
}

#[test]
fn structural_review_compile_interruption_stays_typed_in_detailed_search() {
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let result = evaluate_source("probe();", "fixture.ts", "ts", Some("probe()"), None);
    assert_eq!(result.status, "truncated");
    assert_eq!(result.diagnostics[0].code, "structural.parse.interrupted");
    assert_eq!(result.diagnostics[0].stage, "parse");
}

#[test]
fn structural_review_compile_interruption_keeps_mixed_language_evidence() {
    let root = TmpDir::new();
    fs::write(root.join("one.js"), "probe();").expect("js fixture");
    fs::write(root.join("two.ts"), "probe();").expect("ts fixture");
    let options = || review_file_options(&root, json!({"pattern":"probe()"}), 10);
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let result = search_files_detailed(options()).expect("partial mixed-language search");
    assert_eq!(result.status, "truncated");
    assert_eq!(result.total_matches, 1);
    assert_eq!(result.diagnostics[0].code, "structural.parse.interrupted");
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.contains("not valid syntax"))
    );
    octo::INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.set(true));
    let detailed = search_files_detailed(options()).expect("detailed mixed-language search");
    assert_eq!(detailed.status, "truncated");
    assert_eq!(detailed.total_matches, 1);
    assert_eq!(detailed.diagnostics[0].code, "structural.parse.interrupted");
}

fn write_hit_files(root: &std::path::Path, count: usize) {
    for index in 0..count {
        fs::write(root.join(format!("f{index:05}.ts")), "hit(1);\n").unwrap();
    }
}

/// An unset `max_files` has no file cap: a tree past the old 2,000-file
/// default is scanned whole in one call.
#[test]
fn an_unset_file_cap_scans_the_whole_scope() {
    let root = TmpDir::new();
    write_hit_files(&root, 2_100);
    let result = search_files_detailed(scan_options(&root, 0)).expect("scan");
    assert!(!result.scan_truncated);
    assert_eq!(result.scan_resume, None);
    assert_eq!(result.files.len(), 2_100);
}

/// A scan that runs out of its time budget stops at a chunk boundary and
/// names where to resume; walking the resumes reaches every file exactly once.
#[test]
fn a_scan_past_its_time_budget_resumes_exactly_once() {
    let root = TmpDir::new();
    write_hit_files(&root, 1_300);
    let mut seen = std::collections::BTreeSet::new();
    let mut skip = 0;
    let mut calls = 0;
    loop {
        calls += 1;
        let result = files::search_files_with_budget(
            scan_options(&root, skip),
            StructuralDemand::Matches,
            (&|_| true, None),
            &|_| "ts".to_owned(),
            &|| false,
            Some(std::time::Duration::ZERO),
        )
        .expect("scan");
        for file in &result.files {
            assert!(seen.insert(file.path.clone()), "{} seen twice", file.path);
        }
        match result.scan_resume {
            Some(resume) => {
                assert!(result.scan_truncated);
                assert!(resume > skip, "no progress: {resume} <= {skip}");
                skip = resume;
            }
            None => {
                assert!(!result.scan_truncated);
                break;
            }
        }
    }
    assert_eq!(seen.len(), 1_300);
    assert!(calls > 1, "a zero budget must cut the scan");
}

/// Only an explicit `max_files` caps the walk: an unbounded scan has no
/// walk ceiling and reads every candidate.
#[test]
fn only_an_explicit_max_files_caps_the_file_walk() {
    let root = TmpDir::new();
    for index in 0..6 {
        fs::write(root.join(format!("f{index}.rs")), "fn a() {}\n").expect("write");
    }
    let options = |max_files| StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("fn $N() {}".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: None,
        max_files,
        skip_files: None,
        max_file_bytes: None,
    };
    let capped = search_files_detailed(options(Some(4))).expect("search");
    assert!(capped.scan_truncated);
    assert_eq!(capped.files.len(), 4);
    let whole = search_files_detailed(options(Some(6))).expect("search");
    assert!(!whole.scan_truncated);
    let unbounded = search_files_detailed(options(None)).expect("search");
    assert!(!unbounded.scan_truncated && unbounded.scan_resume.is_none());
    assert_eq!(unbounded.files.len(), 6);
}

/// Default-excluded names are pruned like the text search prunes them: in a
/// git work tree only where git ignores them, so a tracked `build/` is
/// source the walk reads and an ignored `out/build/` is not.
#[test]
fn default_excludes_skip_only_what_git_ignores() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(root.join(".gitignore"), "out/\n").expect("ignore");
    for dir in ["build", "out/build", "src"] {
        fs::create_dir_all(root.join(dir)).expect("dir");
        fs::write(root.join(dir).join("a.ts"), "probe();").expect("file");
    }
    let found = |root: &std::path::Path| {
        let mut options = review_file_options(root, json!({"pattern":"probe()"}), 10);
        options.max_depth = None;
        options.default_exclude_dir = Some(vec!["build".to_owned()]);
        let result = search_files_detailed(options).expect("search");
        let prefix = format!("{}/", root.display());
        matched_paths(&result)
            .iter()
            .map(|path| path.trim_start_matches(&prefix).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(found(&root), ["build/a.ts", "src/a.ts"]);
    fs::remove_dir_all(root.join(".git")).expect("leave the repository");
    // Outside a repository a listed name is pruned by name alone.
    assert_eq!(found(&root), ["src/a.ts"]);
}
