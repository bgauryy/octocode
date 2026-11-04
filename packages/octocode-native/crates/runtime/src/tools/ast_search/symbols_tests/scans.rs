//! Scan cuts, the page memo, `pageSize` and the declarations cache of a
//! directory outline.

use super::*;

/// A `symbols` page for a scan the time budget cut at `resume`.
fn cut_page(resume: u32) -> serde_json::Value {
    let root = Fixture::new();
    let q = query(&root.0, json!({}));
    let set =
        super::super::symbols::SymbolSet::cut_at(root.0.to_string_lossy().into_owned(), resume);
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
pub(super) fn write_settled(path: &std::path::Path, source: &str, age_secs: u64) {
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

/// A directory outline preserves every declaration from its evaluated file.
#[test]
fn a_directory_outline_preserves_the_files_declarations() {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "pub fn shared_outline() {}\npub struct Kept;\n",
    );
    let out = super::super::test_rows::run(&root.0, json!({"operation":"symbols","path":root.0}))
        .expect("symbols");
    assert_eq!(out["filesScanned"], 1, "{out}");
    assert!(declares(&out, "shared_outline"), "{out}");
    assert!(declares(&out, "Kept"), "{out}");
}

#[test]
fn a_fresh_directory_outline_reads_content_even_when_metadata_is_unchanged() {
    let root = Fixture::new();
    let source = root.0.join("lib.ts");
    write_settled(&source, "export function before() {}\n", 0);
    let stamp = std::fs::metadata(&source)
        .expect("metadata")
        .modified()
        .expect("modified");
    let before = empty_outline(&root.0, json!({}));
    assert!(declares(&before, "before"), "{before}");
    std::fs::write(&source, "export function afterx() {}\n").expect("edit same length");
    std::fs::File::options()
        .write(true)
        .open(&source)
        .expect("open")
        .set_modified(stamp)
        .expect("restore stamp");
    let after = empty_outline(&root.0, json!({}));
    assert!(declares(&after, "afterx"), "{after}");
    assert!(!declares(&after, "before"), "{after}");
}
