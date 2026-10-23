//! Directory searches: matches, prefilters, missing roots and per-file limits.

use super::*;

#[test]
fn search_files_finds_matches_and_prefilters_non_matching_files() {
    let root = TmpDir::new();
    fs::write(root.join("a.ts"), "target(value);\n").expect("write a");
    fs::write(root.join("b.ts"), "other(value);\n").expect("write b");
    fs::write(root.join("note.txt"), "target(value);\n").expect("write txt");

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
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("a.ts"));
    assert_eq!(result.skipped_by_pre_filter, 1);
}

/// A scan reports, per file, whether recovered syntax could hide a match:
/// the file whose method a macro attribute garbled, not the clean one.
#[test]
fn search_files_flags_the_file_whose_recovered_syntax_could_hide_a_match() {
    let root = TmpDir::new();
    fs::write(
        root.join("garbled.cpp"),
        "class lexer {\n    JSON_HEDLEY_NON_NULL(2)\n    token_type scan_literal(const char_type* literal_text, const std::size_t length,\n                            token_type return_type)\n    {\n        JSON_ASSERT(char_traits<char_type>::to_char_type(current) == literal_text[0]);\n        return return_type;\n    }\n};\n",
    )
    .expect("write garbled");
    fs::write(
        root.join("clean.cpp"),
        "int f() { JSON_ASSERT(1); return 2; }\n",
    )
    .expect("write clean");
    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("JSON_ASSERT($$$A)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("search files");
    let recovered: Vec<_> = result
        .files
        .iter()
        .filter(|file| file.recovered)
        .map(|file| file.path.rsplit('/').next().unwrap_or(&file.path))
        .collect();
    assert_eq!(recovered, ["garbled.cpp"]);
}

#[test]
fn search_files_errors_on_nonexistent_root_for_every_prefilter_branch() {
    let root = TmpDir::new();
    let missing = root.join("nope");

    // Literal-anchored pattern (`target(` anchor → text prefilter branch),
    // no-anchor pattern (`$FN($$$ARGS)` → walker branch), and a rule. A
    // nonexistent root must be a LOUD error on all of them — the text search yields
    // zero candidates from a missing root without complaining.
    for (pattern, rule) in [
        (Some("target($X)".to_owned()), None),
        (Some("$FN($$$ARGS)".to_owned()), None),
        (None, Some(json!({"pattern":"target($X)"}))),
    ] {
        let result = search_files_detailed(StructuralSearchFilesOptions {
            path: missing.to_string_lossy().to_string(),
            pattern,
            rule,
            include: None,
            exclude_dir: None,
            default_exclude_dir: None,
            exclude: None,
            ignore: crate::search::walk::IgnorePolicy::default(),
            max_depth: None,
            max_files: Some(10),
            skip_files: None,
            max_file_bytes: None,
        });
        match result {
            Ok(ok) => panic!(
                "nonexistent root must error, not return {} matches",
                ok.total_matches
            ),
            Err(err) => assert!(
                err.reason.contains("Cannot access structural search path"),
                "unexpected error text: {err}"
            ),
        }
    }
}

#[test]
fn search_files_respects_excluded_directories_and_large_file_limit() {
    let root = TmpDir::new();
    fs::create_dir_all(root.join("src")).expect("src");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
    fs::write(root.join("src/a.ts"), "target(v);\n").expect("write a");
    fs::write(root.join("src/large.ts"), "target(value);\n").expect("write large");
    fs::write(root.join("node_modules/pkg/b.ts"), "target(value);\n").expect("write b");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: Some(vec!["*.ts".to_owned()]),
        exclude_dir: Some(vec!["node_modules".to_owned()]),
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: Some(14),
    })
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.skipped_large, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert!(matched_paths(&result)[0].ends_with("src/a.ts"));
}

#[test]
fn search_files_reports_non_utf8_and_oversized_sources_per_file() {
    let root = TmpDir::new();
    fs::write(root.join("binary.ts"), b"\xfftarget(x);").expect("write binary");
    fs::write(root.join("large.ts"), "target(value);\n").expect("write large");

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
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: Some(14),
    })
    .expect("search files");

    assert_eq!((result.skipped_unreadable, result.skipped_large), (1, 1));
    assert_eq!(result.status, "truncated");
    let row = |name: &str| {
        result
            .files
            .iter()
            .find(|file| file.path.ends_with(name))
            .expect("file row")
    };
    let binary = row("binary.ts");
    assert_eq!(
        (binary.status.as_str(), binary.skipped_reason.as_deref()),
        ("unreadable", Some("read"))
    );
    assert_eq!(
        binary.diagnostics[0].message,
        "Could not read file content as text: not valid UTF-8 or BOM-marked UTF-16 text."
    );
    assert_eq!(
        binary.diagnostics[0].path.as_deref(),
        Some(binary.path.as_str())
    );
    let large = row("large.ts");
    assert_eq!(
        (large.status.as_str(), large.skipped_reason.as_deref()),
        ("truncated", Some("maxFileBytes"))
    );
    assert_eq!(
        large.diagnostics[0].message,
        "File is 15 bytes, above the structural search limit of 14 bytes."
    );
    assert_eq!(
        large.diagnostics[0].path.as_deref(),
        Some(large.path.as_str())
    );
}

#[test]
fn search_files_accepts_single_file_root() {
    let root = TmpDir::new();
    let file = root.join("a.ts");
    fs::write(&file, "target(value);\n").expect("write file");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: file.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: None,
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("search file");

    assert_eq!(result.total_matches, 1);
    assert_eq!(matched_paths(&result).len(), 1);
}
