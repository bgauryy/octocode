//! A kept scan answers only the same bytes scanned the same way: an edited
//! file at the same path is scanned again, and a repeat equals a fresh scan.
use super::*;
use crate::gh_search_code::lines::kept::scan_kept;

fn hits(bytes: &[u8], keywords: &[&str]) -> FileHits {
    let keywords = keywords.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
    scan_kept(
        bytes,
        "src/kept.rs",
        &keywords,
        "",
        Window::default(),
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
}

fn fresh(bytes: &[u8], keywords: &[&str]) -> FileHits {
    let keywords = keywords.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
    scan(
        bytes,
        "src/kept.rs",
        &keywords,
        "",
        Window::default(),
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
}

#[test]
fn a_repeat_equals_a_fresh_scan_and_an_edit_is_scanned_again() {
    let before = b"fn kept_alpha() {}\nfn other() {}\n";
    let after = b"fn other() {}\n\nfn kept_alpha() {}\n";
    let first = hits(before, &["kept_alpha"]);
    assert_eq!(first, fresh(before, &["kept_alpha"]));
    assert_eq!(hits(before, &["kept_alpha"]), first, "a repeat");
    let edited = hits(after, &["kept_alpha"]);
    assert_eq!(edited, fresh(after, &["kept_alpha"]));
    assert_ne!(edited, first, "the edit moved the hit");
    // Other keywords over the same bytes are their own scan.
    assert_eq!(hits(before, &["other"]), fresh(before, &["other"]));
}

/// A page scanned again is answered from its kept scans, each equal to a
/// fresh scan of its blob, in page order.
#[test]
fn a_page_scanned_again_equals_fresh_scans_in_page_order() {
    let files = (0..3)
        .map(|at| {
            (
                format!(
                    "fn kept_page_{at}_{}() {{}}\nfn other() {{}}\n",
                    std::process::id()
                )
                .into_bytes(),
                format!("src/kept_page_{at}.rs"),
            )
        })
        .collect::<Vec<_>>();
    let page = files
        .iter()
        .map(|(bytes, path)| (bytes.as_slice(), path.as_str()))
        .collect::<Vec<_>>();
    let keywords = vec!["kept_page".to_owned()];
    let listing = Listing {
        keywords: &keywords,
        goal: "",
        window: Window::default(),
    };
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let first = scan_all(&page, &listing, &security, &test_context()).expect("scan");
    let again = scan_all(&page, &listing, &security, &test_context()).expect("scan");
    assert_eq!(again, first);
    for (hits, &(bytes, path)) in first.iter().zip(&page) {
        assert!(matches!(hits, FileHits::Lines { .. }), "{path}");
        assert_eq!(
            *hits,
            scan(bytes, path, &keywords, "", Window::default(), &security),
            "{path}"
        );
    }
}
