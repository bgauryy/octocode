//! Scan cuts, the page memo, `pageSize` and the declarations cache of a
//! directory outline.

use super::*;

/// A `symbols` page for a scan the time budget cut at `resume`.
fn cut_page(resume: u32) -> serde_json::Value {
    let root = Fixture::new();
    let q = query(&root.0, json!({}));
    let set: super::super::symbols::SymbolSet = serde_json::from_value(json!({"path":root.0.to_string_lossy(),"snapshot":"s","declarations":[],"source_descriptors":{"snapshots":[],"representations":[],"units":[]},"diagnostics":[],"files_scanned":resume,"skipped":0,"truncated":true,"scan_resume":resume,"recovered":false,"directory":true}))
    .expect("symbol set");
    let (paths, _) = crate::tools::test_support::simple_policy(&root.0);
    let row_paths = super::super::RowPaths::new(&paths, &root.0, false);
    super::super::symbols::render_page(
        &q,
        &set,
        row_paths,
        None,
        &crate::execution::cancel::NeverCancel,
    )
    .expect("source evidence")
}

/// A time-budget cut resumes with `next.expandScan` wherever it stopped,
/// at and far past the former 100,000-file walk ceiling: no cut is
/// terminal and the resume names the exact offset.
#[test]
fn a_scan_cut_resumes_past_the_former_walk_ceiling() {
    for resume in [1_000, 99_999, 100_000, 100_001, 2_500_000] {
        let page = cut_page(resume);
        let lead = &page["calls"]["expandScan"]["input"]["queries"][0];
        assert_eq!(lead["scanOffset"], json!(resume), "resume {resume}: {page}");
        assert!(
            page.get("terminalLimit").is_none(),
            "resume {resume}: {page}"
        );
        let row = crate::tools::test_support::opened(crate::id::ToolId::AstSearch, lead);
        assert!(
            serde_json::from_value::<octocode_contract_types::AstSearchQuery>(row).is_ok(),
            "resume {resume}: {page}"
        );
    }
}

/// Writes `source` to `path` last modified an hour ago (`age_secs` more), so
/// a scan sees it settled and the page memo may serve a repeated page 1.
fn write_settled(path: &std::path::Path, source: &str, age_secs: u64) {
    std::fs::write(path, source).expect("source");
    let modified = std::time::SystemTime::now() - std::time::Duration::from_secs(3600 + age_secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("set modified");
}

/// A file only `languageGlobs` map to a grammar is part of what the page
/// memo fingerprints: an edit to it is outlined anew, never served from the
/// selection stored before the edit.
#[test]
fn an_edit_to_a_language_globs_file_is_not_served_from_the_page_memo() {
    let root = Fixture::new();
    let mapped = root.0.join("lib.inc");
    let globs = json!({"parserGlobs":{"cpp":["*.inc"]}});
    write_settled(&mapped, "int zzBefore() { return 1; }\n", 0);
    let before = empty_outline(&root.0, globs.clone());
    assert!(declares(&before, "zzBefore"), "{before}");
    write_settled(&mapped, "int zzAfterEdit() { return 2; }\n", 60);
    let after = empty_outline(&root.0, globs);
    assert!(declares(&after, "zzAfterEdit"), "{after}");
    assert!(!declares(&after, "zzBefore"), "{after}");
}

/// A directory outline takes each file's declarations from the one
/// declarations cache, not a second parse memo: a file outlined in its
/// directory is cached for every later reader (its own outline included).
#[test]
fn a_directory_outline_parses_through_the_declarations_cache() {
    let root = Fixture::new();
    let source = "pub fn shared_outline() {}\npub struct Kept;\n";
    root.write("src/lib.rs", source);
    let out = super::super::test_rows::run(&root.0, json!({"operation":"symbols","path":root.0}))
        .expect("symbols");
    assert_eq!(out["filesScanned"], 1, "{out}");
    let file = root.0.join("src/lib.rs").canonicalize().expect("canonical");
    // Residency in the shared, bounded cache is not stable under parallel
    // tests; that the parse went through it is.
    assert!(crate::tools::declarations_cache::parsed_through_cache(
        source,
        &file.to_string_lossy(),
        None
    ));
}
