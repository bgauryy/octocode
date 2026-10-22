use super::*;

use std::path::Path;

/// Walks `options` and scans what the walk finds, with the time budget
/// `budget` (`None` reads every candidate).
fn scan_with_budget<T: Send>(
    options: GraphFactsScanOptions,
    admit: &(dyn Fn(&Path, Option<fs::FileType>) -> bool + Sync),
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    budget: Option<std::time::Duration>,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    let walked = walk_graph_fact_files(&options, admit, stop)?;
    evaluate_walk(options, walked, FactsSource::Graph, keep, map, stop, budget)
}

/// A scan keeping every parsed file whole.
#[derive(Debug)]
struct Scanned {
    entries: Vec<GraphFactsTypedEntry>,
    skipped: Vec<GraphFactsScanDiagnostic>,
    files_skipped: u32,
    truncated: bool,
}

/// Every file the scan read or skipped, sorted.
fn scanned_paths(result: &Scanned) -> Vec<String> {
    let mut paths: Vec<String> = result
        .entries
        .iter()
        .map(|entry| entry.relative_path.clone())
        .chain(result.skipped.iter().map(|skip| skip.relative_path.clone()))
        .collect();
    paths.sort_unstable();
    paths
}

fn scan_admitted(
    options: GraphFactsScanOptions,
    admit: &(dyn Fn(&Path, Option<fs::FileType>) -> bool + Sync),
) -> Result<Scanned, GraphScanError> {
    let scan = scan_with_budget(
        options,
        admit,
        &|_| true,
        &|view| Some(view.to_entry()),
        &|| false,
        Some(crate::scan_budget::SCAN_TIME_BUDGET),
    )?;
    Ok(Scanned {
        entries: scan.kept.into_iter().map(|(_, entry)| entry).collect(),
        skipped: scan.skipped,
        files_skipped: scan.files_skipped,
        truncated: scan.truncated,
    })
}

fn scan_graph_facts(options: GraphFactsScanOptions) -> Result<Scanned, GraphScanError> {
    scan_admitted(options, &|_, _| true)
}

fn create_supported_files(root: &Path, count: usize) {
    fs::create_dir_all(root).expect("create fixture");
    for index in 0..count {
        fs::write(
            root.join(format!("entry-{index}.ts")),
            format!("export const value{index} = {index};"),
        )
        .expect("write fixture file");
    }
}

/// A just-written file scans with no stamp: its digest stays the proof. On
/// the settle clock of a thread that settled it, the scan stamps it.
#[test]
fn a_scan_stamps_only_settled_files() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("create fixture");
    fs::write(root.join("a.ts"), "export const a = 1;").expect("write fixture");
    let scan = || {
        scan_graph_facts(GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(10),
            skip_files: None,
            ..Default::default()
        })
        .expect("scan graph facts")
    };
    let result = scan();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].source_stamp, None);
    crate::stamp::settle_written_so_far();
    let result = scan();
    assert_eq!(result.entries[0].source_stamp.map(|s| s.size), Some(19));
}

/// A walk cut at its file cap names the first candidate it left out, and
/// every file before it is in the walk: a lowered walk ceiling cuts an
/// unbounded scan the same way an explicit `max_files` does.
#[test]
fn a_capped_walk_names_the_first_candidate_it_left_out() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 5);
    let walk = |options: GraphFactsScanOptions| {
        walk_graph_fact_files(&options, &|_, _| true, &|| false).expect("walk")
    };
    let options = |max_files| GraphFactsScanOptions {
        path: path_string(&root),
        max_files,
        ..Default::default()
    };
    let capped = walk(options(Some(3)));
    let names = |paths: &[String]| {
        paths
            .iter()
            .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
            .collect::<Vec<_>>()
    };
    let listed: Vec<String> = capped.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(names(&listed), ["entry-0.ts", "entry-1.ts", "entry-2.ts"]);
    let frontier = capped.first_unlisted.expect("a cut walk has a frontier");
    assert_eq!(names(&[frontier]), ["entry-3.ts"]);
    assert_eq!(walk(options(Some(5))).first_unlisted, None);
    let _ceiling = crate::scan_budget::lower_walk_ceiling(2);
    let unbounded = walk(options(None));
    assert_eq!(unbounded.entries.len(), 2);
    assert_eq!(
        names(&[unbounded.first_unlisted.expect("frontier")]),
        ["entry-2.ts"]
    );
    let scanned = scan_with_budget(
        options(None),
        &|_, _| true,
        &|_| true,
        &|view| Some(view.relative_path.to_owned()),
        &|| false,
        None,
    )
    .expect("scan");
    assert!(scanned.truncated);
    assert_eq!(scanned.scan_resume, Some(2));
    assert_eq!(
        names(&[scanned.scan_frontier.expect("scan frontier")]),
        ["entry-2.ts"]
    );
}

/// An unset `max_files` caps nothing but the time: a tree past the old
/// 20,000-file default is read whole; a scan past its budget stops at a
/// chunk boundary and the resumes reach every file exactly once.
#[test]
fn an_unbounded_scan_resumes_exactly_once_past_its_time_budget() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 1_300);
    let scan = |skip: u32, budget: Option<std::time::Duration>| {
        scan_with_budget(
            GraphFactsScanOptions {
                path: path_string(&root),
                max_files: None,
                skip_files: Some(skip),
                ..Default::default()
            },
            &|_, _| true,
            &|_| true,
            &|view| Some(view.relative_path.to_owned()),
            &|| false,
            budget,
        )
        .expect("scan graph facts")
    };
    let whole = scan(0, None);
    assert!(!whole.truncated);
    assert_eq!(whole.scan_resume, None);
    assert_eq!(whole.kept.len(), 1_300);

    let mut seen = std::collections::BTreeSet::new();
    let (mut skip, mut calls) = (0, 0);
    loop {
        calls += 1;
        let result = scan(skip, Some(std::time::Duration::ZERO));
        for (path, _) in &result.kept {
            assert!(seen.insert(path.clone()), "{path} twice");
        }
        let Some(resume) = result.scan_resume else {
            assert!(!result.truncated);
            break;
        };
        assert!(result.truncated && resume > skip);
        skip = resume;
    }
    assert_eq!(seen.len(), 1_300);
    assert!(calls > 1);
}

/// A mapped scan keeps only what its map returns, so a scan over a huge
/// tree holds the few rows the caller wants, not every file's facts.
#[test]
fn a_mapped_scan_keeps_only_what_the_map_returns() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 40);
    let scan = scan_with_budget(
        GraphFactsScanOptions {
            path: path_string(&root),
            ..Default::default()
        },
        &|_, _| true,
        &|_| true,
        &|view| {
            view.facts
                .declarations
                .iter()
                .find(|declaration| declaration.name == "value7")
                .map(|declaration| declaration.name.clone())
        },
        &|| false,
        Some(crate::scan_budget::SCAN_TIME_BUDGET),
    )
    .expect("mapped scan");
    assert_eq!(scan.parsed, 40, "every file is parsed and counted");
    assert_eq!(scan.kept, [("entry-7.ts".to_owned(), "value7".to_owned())]);
    assert!(!scan.truncated);
}

/// The facts cache weighs estimated heap, not source bytes: facts of a
/// small file with many declarations outweigh the file several times.
#[test]
fn cached_facts_are_weighed_by_their_heap_not_their_source() {
    let source: String = (0..200)
        .map(|index| format!("export const v{index} = {index};\n"))
        .collect();
    let extraction = crate::signatures::extract_graph_facts_with_metadata_inner(&source, "a.ts")
        .expect("extract facts");
    let weight = facts_heap_bytes(&extraction.facts, &extraction.reference_counts);
    assert!(
        weight > source.len() * 4,
        "{weight} bytes for {} source bytes",
        source.len()
    );
}

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
    fs::write(root.join("invalid.js"), [0xff, 0xfe]).expect("write invalid utf8 fixture");
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

#[test]
fn reports_truncation_only_when_graph_scan_exceeds_the_file_cap() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 3);

    let below_cap = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(4),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan below cap");
    assert_eq!(below_cap.entries.len(), 3);
    assert!(!below_cap.truncated);

    let exact_cap = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(3),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan at cap");
    assert_eq!(exact_cap.entries.len(), 3);
    assert!(!exact_cap.truncated);

    let over_cap = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(2),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan over cap");
    assert_eq!(over_cap.entries.len(), 2);
    assert!(over_cap.truncated);
}

#[test]
fn detects_graph_scan_overflow_across_directory_boundaries() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    // Each child ends exactly at the initial cap, whichever is visited first.
    create_supported_files(&root.join("left"), 1);
    create_supported_files(&root.join("right"), 1);

    let capped = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(1),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan nested graph at cap");
    assert_eq!(capped.entries.len(), 1);
    assert!(
        capped.truncated,
        "the sibling subtree still has a graph file"
    );

    let expanded = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(2),
        skip_files: None,
        ..Default::default()
    })
    .expect("expand nested graph scan");
    assert_eq!(
        scanned_paths(&expanded),
        ["left/entry-0.ts", "right/entry-0.ts"]
    );
    assert!(
        !expanded.truncated,
        "an exactly full complete scan has no overflow"
    );
}

#[test]
fn ignored_files_do_not_make_an_exact_graph_cap_partial() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 1);
    fs::write(root.join("ignored.txt"), "not graph input").expect("write ignored file");
    fs::create_dir_all(root.join("ignored-directory")).expect("create ignored subtree");
    fs::write(
        root.join("ignored-directory/ignored.txt"),
        "not graph input",
    )
    .expect("write ignored child");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(1),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan exactly one matching graph file");
    assert_eq!(scanned_paths(&result), ["entry-0.ts"]);
    assert!(
        !result.truncated,
        "nonmatching entries are not graph overflow"
    );
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn policy_denied_graph_files_do_not_consume_the_scan_budget() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(root.join("private")).expect("fixture");
    fs::write(root.join("private/hidden.rs"), "pub fn hidden() {}").expect("hidden");
    fs::write(root.join("visible.rs"), "pub fn visible() {}").expect("visible");
    let result = scan_admitted(
        GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(1),
            skip_files: None,
            ..Default::default()
        },
        &|path, _| !path.starts_with(root.join("private")),
    )
    .expect("filtered scan");
    assert_eq!(scanned_paths(&result), ["visible.rs"]);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.files_skipped, 0);
    assert!(!result.truncated);
}

#[test]
fn graph_read_enforces_limit_when_file_grows_after_discovery() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("fixture");
    let file = root.join("growing.rs");
    fs::write(&file, "pub fn initial() {}").expect("initial");
    // Walked after `growing.rs` (name order): it grows once listed.
    let later = root.join("z.rs");
    fs::write(&later, "pub fn later() {}").expect("later");
    let visits = AtomicUsize::new(0);
    let result = scan_admitted(
        GraphFactsScanOptions {
            path: path_string(&root),
            max_file_bytes: Some(32),
            ..Default::default()
        },
        &|path, _| {
            if path == later && visits.fetch_add(1, Ordering::SeqCst) == 0 {
                fs::write(&file, "x".repeat(65_536)).expect("concurrent growth");
            }
            true
        },
    )
    .expect("bounded scan");
    assert_eq!(scanned_paths(&result), ["growing.rs", "z.rs"]);
    assert_eq!(result.skipped[0].relative_path, "growing.rs");
    assert_eq!(result.skipped[0].code, "graph.scan.fileTooLarge");
}
