//! What a scan extracts: facts, reference counts, parser overrides and failures.

use super::*;

#[test]
fn scans_supported_files_and_counts_export_references() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(root.join("src")).expect("create fixture");
    fs::write(
        root.join("src/entry.ts"),
        "export const answer = 1; console.log(answer);",
    )
    .expect("write fixture");
    fs::write(root.join("src/large.ts"), "x".repeat(64)).expect("write oversized fixture");
    fs::write(root.join("README.md"), "# ignored").expect("write ignored file");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: Some(48),
        ..Default::default()
    })
    .expect("scan graph facts");

    assert_eq!(result.entries.len(), 1);
    assert_eq!(scanned_paths(&result), ["src/entry.ts", "src/large.ts"]);
    assert_eq!(result.files_skipped, 1);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].relative_path, "src/large.ts");
    assert_eq!(result.skipped[0].code, "graph.scan.fileTooLarge");
    assert_eq!(result.entries[0].relative_path, "src/entry.ts");
    assert!(
        result.entries[0].reference_counts[0]
            .declaration_id
            .contains("#answer@")
    );
    assert_eq!(result.entries[0].reference_counts[0].count, 1);
    assert!(!result.truncated);
}

/// A panic in any per-file step, here the caller's `keep` prefilter outside
/// extraction, is that file's named skip: files panicking at once on
/// several workers each get theirs, and the scan returns with the rest.
/// On Unix `\` is a file-name byte: `f\g.ts` is its own file beside
/// `f/g.ts`, in entries and in skip rows alike.
#[cfg(unix)]
#[test]
fn a_backslash_in_a_unix_file_name_stays_in_its_rows() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(root.join("f")).expect("create fixture");
    fs::write(root.join("f/g.ts"), "export const a = 1;").expect("write fixture");
    fs::write(root.join("f\\g.ts"), "x".repeat(64)).expect("write fixture");
    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_file_bytes: Some(48),
        ..Default::default()
    })
    .expect("scan graph facts");
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].relative_path, "f/g.ts");
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].relative_path, "f\\g.ts");
}

#[test]
fn panics_on_many_files_at_once_are_each_that_files_named_skip() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("create fixture");
    for index in 0..48 {
        let marker = if index % 2 == 0 {
            "keep-panics"
        } else {
            "fine"
        };
        fs::write(
            root.join(format!("f{index:02}.ts")),
            format!("// {marker}\nexport const value{index} = {index};"),
        )
        .expect("write fixture file");
    }
    let scan = scan_with_budget(
        GraphFactsScanOptions {
            path: path_string(&root),
            ..Default::default()
        },
        &|_, _| true,
        &|source| {
            assert!(!source.contains("keep-panics"), "injected keep panic");
            true
        },
        &|view| Some(view.to_entry()),
        &|| false,
        None,
    )
    .expect("the scan returns");
    assert_eq!(scan.kept.len(), 24);
    assert_eq!(scan.files_skipped, 24);
    let mut named = scan
        .skipped
        .iter()
        .map(|skip| {
            assert_eq!(skip.code, PANICKED, "{skip:?}");
            assert!(skip.message.contains("keep phase panicked"), "{skip:?}");
            assert!(!skip.message.contains("injected"), "{skip:?}");
            skip.relative_path.clone()
        })
        .collect::<Vec<_>>();
    named.sort_unstable();
    let expected = (0..48)
        .step_by(2)
        .map(|index| format!("f{index:02}.ts"))
        .collect::<Vec<_>>();
    assert_eq!(named, expected);
}

/// The one named skip of each file of `scan` that panicked, with the phase
/// its message names.
fn panicked_files<T>(scan: &GraphFactsMappedScan<T>) -> Vec<(String, String)> {
    scan.skipped
        .iter()
        .map(|skip| {
            assert_eq!(skip.code, PANICKED, "{skip:?}");
            assert!(!skip.message.contains("injected"), "{skip:?}");
            let phase = skip
                .message
                .split(" phase panicked")
                .next()
                .unwrap_or_default()
                .to_owned();
            (skip.relative_path.clone(), phase)
        })
        .collect()
}

/// A panic weighing what the caller kept of a file is that file's named
/// skip in the weigh phase: the file is not kept, the others are.
#[test]
fn a_weigh_panic_is_that_files_skip_in_the_weigh_phase() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    for name in ["a.ts", "heavy.ts", "z.ts"] {
        fs::write(root.join(name), "export const value = 1;\n").expect("write fixture");
    }
    let options = GraphFactsScanOptions {
        path: path_string(&root),
        ..Default::default()
    };
    let walked = walk_graph_fact_files(&options, &|_, _| true, &|| false).expect("walk");
    let weigh = |path: &String| {
        assert!(path != "heavy.ts", "injected weigh panic");
        1
    };
    let scan = scan_walked_graph_facts_within(
        options,
        walked,
        FactsSource::Graph,
        &|_| true,
        &|view| Some(view.relative_path.to_owned()),
        &|| false,
        &ScanHold {
            limit: usize::MAX,
            weigh: &weigh,
        },
    )
    .expect("the scan returns");
    assert_eq!(
        panicked_files(&scan),
        [("heavy.ts".to_owned(), "weigh".to_owned())]
    );
    let kept = scan
        .kept
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kept, ["a.ts", "z.ts"]);
}

/// A panic admitting a file's bytes as text is that file's named skip in
/// the decode phase.
#[test]
fn a_decode_panic_is_that_files_skip_in_the_decode_phase() {
    const MARKER: &str = "decode-panics-b94";
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    for name in ["a.ts".to_owned(), format!("{MARKER}.ts")] {
        fs::write(root.join(name), "export const value = 1;\n").expect("write fixture");
    }
    super::super::evaluate::DECODE_FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(MARKER);
    let scan = scan_with_budget(
        GraphFactsScanOptions {
            path: path_string(&root),
            ..Default::default()
        },
        &|_, _| true,
        &|_| true,
        &|view| Some(view.relative_path.to_owned()),
        &|| false,
        None,
    )
    .expect("the scan returns");
    assert_eq!(
        panicked_files(&scan),
        [(format!("{MARKER}.ts"), "decode".to_owned())]
    );
    assert_eq!(scan.kept.len(), 1);
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn path_scoped_cpp_header_override_changes_only_matching_headers() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(root.join("include")).expect("create include fixture");
    fs::create_dir_all(root.join("legacy")).expect("create legacy fixture");
    fs::write(
        root.join("include/widget.h"),
        "namespace Space { class Widget {}; }\n",
    )
    .expect("write C++ header");
    fs::write(root.join("legacy/plain.h"), "struct Plain { int x; };\n").expect("write C header");
    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        language_globs: Some(vec![crate::types::GraphLanguageGlob {
            language: "cpp".into(),
            glob: "include/**/*.h".into(),
        }]),
        ..Default::default()
    })
    .expect("scan with parser map");
    assert_eq!(result.entries.len(), 2);
    let declares = |path: &str, name: &str| {
        result
            .entries
            .iter()
            .find(|entry| entry.relative_path == path)
            .is_some_and(|entry| entry.facts.declarations.iter().any(|row| row.name == name))
    };
    assert!(declares("include/widget.h", "Widget"));
    assert!(declares("legacy/plain.h", "Plain"));
}

#[test]
fn malformed_exclude_glob_fails_before_scanning_files() {
    let error = scan_graph_facts(GraphFactsScanOptions {
        path: "/nonexistent".into(),
        exclude: Some(vec!["**/dist/**".into(), "[bad".into()]),
        ..Default::default()
    })
    .expect_err("malformed exclude glob");
    assert!(matches!(error, GraphScanError::InvalidGlob(_)), "{error:?}");
    assert!(
        error
            .to_string()
            .starts_with("[ast.language.invalidGlob] invalid exclude glob '[bad': "),
        "{error}"
    );
}

#[test]
fn malformed_parser_glob_fails_before_scanning_files() {
    let error = scan_graph_facts(GraphFactsScanOptions {
        path: "/nonexistent".into(),
        language_globs: Some(vec![crate::types::GraphLanguageGlob {
            language: "c".into(),
            glob: "[".into(),
        }]),
        ..Default::default()
    })
    .expect_err("malformed glob");
    assert!(matches!(error, GraphScanError::InvalidGlob(_)), "{error:?}");
    assert!(
        error
            .to_string()
            .starts_with("[ast.language.invalidGlob] invalid languageGlobs glob '[': "),
        "{error}"
    );
    let unsupported = scan_graph_facts(GraphFactsScanOptions {
        path: "/nonexistent".into(),
        language_globs: Some(vec![crate::types::GraphLanguageGlob {
            language: "no-such-grammar".into(),
            glob: "*.x".into(),
        }]),
        ..Default::default()
    })
    .expect_err("unknown grammar");
    assert!(
        matches!(unsupported, GraphScanError::UnsupportedLanguage(_)),
        "{unsupported:?}"
    );
}

#[test]
fn reports_a_stable_read_failure_and_extracts_past_any_parse_size() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("create fixture");
    // Not [0xff, 0xfe]: that is a UTF-16 byte-order mark, which is admitted.
    fs::write(root.join("invalid.js"), [0xc3, 0x28]).expect("write invalid utf8 fixture");
    // No parse size refuses a file the scan admits: `max_file_bytes` alone
    // bounds what it reads.
    let extraction_size = crate::security::MAX_CONTENT_SIZE;
    fs::write(root.join("extract.ts"), vec![b'x'; extraction_size])
        .expect("write extraction fixture");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: Some(extraction_size as u64),
        ..Default::default()
    })
    .expect("scan graph facts");

    assert_eq!(result.files_skipped as usize, result.skipped.len());
    assert_eq!(
        result
            .skipped
            .iter()
            .map(|diagnostic| (diagnostic.relative_path.as_str(), diagnostic.code.as_str()))
            .collect::<Vec<_>>(),
        [("invalid.js", "graph.scan.readFailed")]
    );
}

#[test]
fn preserves_reference_counts_from_both_fact_producers() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("create fixture");
    fs::write(
        root.join("main.ts"),
        "export function alpha() { return alpha; }",
    )
    .expect("write TypeScript fixture");
    fs::write(root.join("lib.rs"), "pub fn beta() { let _ = beta; }").expect("write Rust fixture");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        ..Default::default()
    })
    .expect("scan graph facts");

    assert_eq!(
        result
            .entries
            .iter()
            .map(|entry| (
                entry.relative_path.as_str(),
                entry.reference_counts[0]
                    .declaration_id
                    .split(['#', '@'])
                    .nth(1)
                    .unwrap_or_default(),
                entry.reference_counts[0].count,
            ))
            .collect::<Vec<_>>(),
        // The declaration name token is not a reference; the value use is.
        [("lib.rs", "beta", 1), ("main.ts", "alpha", 1)]
    );
}

#[test]
fn typed_scan_preserves_structural_ids_ranges_edges_and_content_receipt() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("create fixture");
    fs::write(
        root.join("lib.rs"),
        "pub fn alpha() { beta(); }\nfn beta() {}",
    )
    .expect("write fixture");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        ..Default::default()
    })
    .expect("typed scan");
    let entry = result.entries.first().expect("entry");
    assert_eq!(entry.facts.file, "lib.rs");
    assert!(!entry.content_digest.is_empty());
    assert!(
        entry
            .facts
            .declarations
            .iter()
            .all(|decl| !decl.id.is_empty())
    );
    assert!(
        entry
            .facts
            .declarations
            .iter()
            .all(|decl| decl.range.end >= decl.range.start)
    );
    assert!(entry.facts.edges.iter().all(|edge| !edge.id.is_empty()));
}
