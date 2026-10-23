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
            language: "cpp".into(),
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
fn reports_stable_read_and_extraction_failures() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("create fixture");
    // Not [0xff, 0xfe]: that is a UTF-16 byte-order mark, which is admitted.
    fs::write(root.join("invalid.js"), [0xc3, 0x28]).expect("write invalid utf8 fixture");
    let extraction_size = crate::signatures::MAX_PARSE_SIZE + 1;
    fs::write(root.join("extract.ts"), vec![b'x'; extraction_size])
        .expect("write extraction fixture");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: Some(extraction_size as u32),
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
        [
            ("extract.ts", "graph.scan.extractFailed"),
            ("invalid.js", "graph.scan.readFailed"),
        ]
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
