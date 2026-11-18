use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::ContentSecurity;
use std::path::Path;

/// A raw selection needs its source-line index to place byte windows.
/// Keeping the index lets every continuation reuse the first scan's
/// allocation instead of building a source-sized table for each page.
#[test]
fn selection_pages_reuse_the_scanned_source_line_index() {
    let source = (0..512)
        .map(|line| format!("let retained_line_{line} = {line};\n"))
        .collect::<String>();
    let path = Path::new("/fixture/retained_source_index.rs");
    let q: LocalFetchQuery = serde_json::from_value(serde_json::json!({
        "path": path,
        "ranges": [{"startLine": 100, "endLine": 400}],
        "limit": 2,
    }))
    .expect("read query");
    let facts = SourceFacts::default();
    let security = ContentSecurity::new();
    let first = super::super::view::process_fetched_content(
        &q,
        source.as_bytes(),
        path,
        &facts,
        &security,
        &NeverCancel,
    );
    assert!(first.error.is_none(), "{first:?}");
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("selection spans several pages");
    let sha256 = octocode_engine::digest::sha256(source.as_bytes());
    let paths = (path, "retained_source_index.rs");
    let scanned = ScannedSource::of(&q, &sha256, paths, &facts, &security)
        .expect("range scan")
        .get(&q)
        .expect("scan retained");
    let kept = Retention::of(&q, &sha256, paths, &facts, &security)
        .expect("view retention")
        .replay(&next.query)
        .expect("view retained");
    assert!(Arc::ptr_eq(
        scanned.offsets.as_ref().expect("scanned source index"),
        kept.source.offsets.as_ref().expect("retained source index"),
    ));
    forget_view(&sha256);
    forget_scan(&sha256);
}
