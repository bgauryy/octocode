//! Scan stamps, walk caps, time budgets, mapped scans and the facts cache weight.

use super::*;

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
