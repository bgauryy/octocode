//! The file walker: globs, ignore files, rule prefilters and scope options.

use super::*;

// ── native walker: recursive globs (#6), ignore semantics (#7), rule
//    prefilter (#9) ─────────────────────────────────────────────────────

#[test]
fn search_files_supports_recursive_glob_includes() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src/nested")).expect("nested dir");
    fs::write(root.join("src/a.ts"), "target(v);\n").expect("a");
    fs::write(root.join("src/nested/b.ts"), "target(v);\n").expect("b");
    fs::write(root.join("src/c.js"), "target(v);\n").expect("c");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: Some(vec!["src/**/*.ts".to_owned()]),
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("glob search");

    assert_eq!(
        matched_paths(&result).len(),
        2,
        "both nested .ts match; .js excluded"
    );
    assert!(
        matched_paths(&result)
            .iter()
            .all(|path| path.ends_with(".ts"))
    );
}

#[test]
fn search_files_honors_dot_ignore_files() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("skip")).expect("skip dir");
    fs::write(root.join(".ignore"), "skip/\n").expect("ignore file");
    fs::write(root.join("keep.ts"), "target(v);\n").expect("keep");
    fs::write(root.join("skip/x.ts"), "target(v);\n").expect("skipped");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("ignore search");

    assert_eq!(matched_paths(&result).len(), 1, ".ignore skips skip/");
    assert!(matched_paths(&result)[0].ends_with("keep.ts"));
}

#[test]
fn search_files_prefilters_rule_by_inner_pattern() {
    let root = TmpDir::new();
    fs::write(
        root.join("has.ts"),
        "async function f() {\n  await g();\n}\n",
    )
    .expect("has");
    fs::write(root.join("none.ts"), "function f() {\n  return 1;\n}\n").expect("none");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: None,
        rule: Some(json!({"pattern":"await $C"})),
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("rule search");

    // Anchor "await" lets none.ts skip parsing entirely.
    assert_eq!(result.skipped_by_pre_filter, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("has.ts"));
}

#[test]
fn search_files_operator_only_pattern_parses_every_candidate() {
    let root = TmpDir::new();
    fs::write(root.join("match.js"), "foo && foo();\n").expect("match");
    fs::write(root.join("nomatch.js"), "foo || foo();\n").expect("nomatch");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("$A && $A()".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("operator anchor search");

    assert_eq!(result.skipped_by_pre_filter, 0);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("match.js"));
}

#[test]
fn operator_spacing_does_not_split_directory_and_file_results() {
    let root = TmpDir::new();
    let source = "if (a && !b) { run(); }\nx = y - 1;\n";
    fs::write(root.join("spaced.js"), source).expect("spaced");
    for pattern in ["$A&&!$B", "$A && !$B", "$A=$B-1"] {
        let file_matches = search(source, "js", Some(pattern), None).expect("file search");
        let result = search_files_detailed(StructuralSearchFilesOptions {
            path: root.to_string_lossy().to_string(),
            pattern: Some(pattern.to_owned()),
            rule: None,
            include: None,
            exclude_dir: None,
            default_exclude_dir: None,
            exclude: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: Some(50),
            skip_files: None,
            max_file_bytes: None,
        })
        .expect("directory search");
        assert_eq!(result.skipped_by_pre_filter, 0, "{pattern}");
        let dir_count = result.files.iter().map(|f| f.matches.len()).sum::<usize>();
        assert_eq!(dir_count, file_matches.len(), "{pattern}");
    }
    assert_eq!(
        search(source, "js", Some("$A&&!$B"), None)
            .expect("file")
            .len(),
        1
    );
}

// ── scope parity: exclude / hidden / no_ignore / max_depth ───────────────
// Local search defines `exclude`/`hidden`/`noIgnore`/`maxDepth`; the
// structural lane must honor them like the text/regex lane.

fn write_scope_fixture(root: &std::path::Path) {
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    fs::write(root.join("excluded.ts"), "target(value);\n").expect("excluded");
    fs::write(root.join(".hidden.ts"), "target(value);\n").expect("hidden");
    fs::create_dir_all(root.join("nested")).expect("nested");
    fs::write(root.join("nested/deep.ts"), "target(value);\n").expect("deep");
    // A .gitignore that excludes gitignored.ts — proves `no_ignore` unlocks it.
    fs::write(root.join(".gitignore"), "gitignored.ts\n").expect("gitignore");
    fs::write(root.join("gitignored.ts"), "target(value);\n").expect("gitignored");
}

fn scope_result_paths(options: StructuralSearchFilesOptions) -> Vec<String> {
    let result = search_files_detailed(options).expect("scope search");
    let mut paths: Vec<String> = matched_paths(&result)
        .into_iter()
        .map(|path| {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    paths.sort();
    paths
}

/// Each scope option changes which files a kind rule parses. `rule:kind`
/// has no literal anchor, so every supported .ts is parsed and the option
/// under test is the only thing that adds or drops a file.
#[test]
fn structural_files_honor_scope_options() {
    type Scope = fn(&mut StructuralSearchFilesOptions);
    let cases: [(&str, Scope, &[&str], &[&str]); 4] = [
        (
            "exclude glob drops excluded.ts",
            |o| o.exclude = Some(vec!["excluded.ts".to_owned()]),
            &["match.ts"],
            &["excluded.ts"],
        ),
        // Default (hidden:None) ignores dot-files.
        (
            "hidden:Some(true) includes .hidden.ts",
            |o| o.ignore.include_hidden = true,
            &[".hidden.ts"],
            &[],
        ),
        // .gitignore excludes gitignored.ts.
        (
            "no_ignore:Some(true) includes the .gitignored file",
            |o| o.ignore.include_ignored = true,
            &["gitignored.ts"],
            &[],
        ),
        // max_depth:1 = root only.
        (
            "max_depth:1 does not descend into nested/",
            |o| o.max_depth = Some(1),
            &["match.ts"],
            &["deep.ts"],
        ),
    ];
    for (case, scope, present, absent) in cases {
        let root = TmpDir::new();
        write_scope_fixture(&root);
        let mut options = StructuralSearchFilesOptions {
            path: root.to_string_lossy().to_string(),
            rule: Some(json!({"kind":"call_expression"})),
            pattern: None,
            include: None,
            exclude: None,
            exclude_dir: None,
            default_exclude_dir: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: Some(50),
            skip_files: None,
            max_file_bytes: None,
        };
        scope(&mut options);
        let paths = scope_result_paths(options);
        for name in present {
            assert!(
                paths.iter().any(|p| p == name),
                "{case}: {name} present: got {paths:?}"
            );
        }
        for name in absent {
            assert!(
                !paths.iter().any(|p| p == name),
                "{case}: {name} absent: got {paths:?}"
            );
        }
    }
}

#[test]
fn structural_anchor_prefilter_prunes_beyond_max_depth_before_search() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("nested")).expect("nested");
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    fs::write(root.join("nested/noanchor.ts"), "other(value);\n").expect("deep");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        // Structural native depth counts the root as 0 and root files as 1.
        max_depth: Some(1),
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("structural search");

    assert_eq!(result.parsed_files, 1);
    assert_eq!(
        result.skipped_by_pre_filter, 0,
        "the anchor prefilter must not scan the nested file before depth filtering"
    );
}
