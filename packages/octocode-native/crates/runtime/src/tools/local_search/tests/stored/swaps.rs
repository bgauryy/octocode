//! Stamp-preserving swaps and rewrites: a stored page never serves values
//! or files the read policy, the stored digests or the stored redaction no
//! longer vouch for.
use super::*;
use crate::tools::local_search::manifest;

/// A matched file swapped for a symlink out of the root, under its stored
/// modification time and digest, drops the stored scan (its directory and
/// its own stamp changed). The page that would show it rescans, and the
/// rescan's walk, which follows no link, no longer finds the file: the
/// snapshot restarts.
#[cfg(unix)]
#[test]
fn a_swapped_matched_file_drops_the_stored_scan_and_restarts_its_page() {
    let root = tempfile::tempdir().expect("fixture directory");
    let outside = tempfile::tempdir().expect("outside directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["needle"]}),
        None,
    );
    let snapshot = search_in(&request, &policy, &security, Some(6_000))
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let page = |page: u32| {
        ls_query(
            serde_json::json!({"snapshot": snapshot, "page": page}),
            Some(&request),
        )
    };
    assert!(manifest::get(&snapshot, &policy.identity()).is_some());
    let on_three = page_paths(&page(3), &policy, &security);
    swap_for_outside_copy(&root.path().join(&on_three[0]), outside.path());
    assert!(
        manifest::get(&snapshot, &policy.identity()).is_none(),
        "the swap drops the stored scan"
    );
    let failed = search_in(&page(3), &policy, &security, Some(6_000))
        .expect_err("page 3 would show the swapped file");
    assert_eq!(failed.code, ErrorCode::StaleSnapshot, "{}", failed.message);
}

/// A stored page checks the read policy of every file it shows, even for a
/// swap that lands after the page took its stored scan (when the stored
/// walk's proof still held): a shown file swapped for a symlink out of the
/// root, under its stored stamp and digest, fails the page. A swapped file
/// the page neither reads nor shows leaves the page as it was; the page
/// that shows it fails.
#[cfg(unix)]
#[test]
fn a_stored_page_checks_the_read_policy_of_the_files_it_shows() {
    // One settled fixture per swap: a swap drops its own stored scan.
    let roots = [(); 3].map(|()| {
        let root = tempfile::tempdir().expect("fixture directory");
        paged_fixture(root.path());
        root
    });
    settle();
    let stored = |root: &std::path::Path| {
        let (policy, security) = policy_for(root);
        let request = ls_query(
            serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": ["needle"]}),
            None,
        );
        let snapshot = search_in(&request, &policy, &security, Some(6_000))
            .expect("page 1")
            .source_snapshot
            .expect("snapshot");
        assert!(manifest::get(&snapshot, &policy.identity()).is_some());
        let page = move |page: u32| {
            ls_query(
                serde_json::json!({"snapshot": snapshot, "page": page}),
                Some(&request),
            )
        };
        (policy, security, page)
    };
    let swapped_page = |root: &std::path::Path, file: &str, shown: u32| {
        let (policy, security, page) = stored(root);
        let outside = tempfile::tempdir().expect("outside directory");
        stored_page_after(&page(shown), &policy, &security, Some(6_000), || {
            swap_for_outside_copy(&root.join(file), outside.path());
        })
    };
    let (policy, security, page) = stored(roots[0].path());
    let second = serde_json::to_string(
        &search_in(&page(2), &policy, &security, Some(6_000)).expect("page 2"),
    )
    .expect("json");
    let on_two = page_paths(&page(2), &policy, &security);
    let on_three = page_paths(&page(3), &policy, &security);
    let later = on_three
        .iter()
        .find(|path| !on_two.contains(path))
        .expect("a page-3 file");
    let unchanged =
        swapped_page(roots[0].path(), later, 2).expect("page 2 neither reads nor shows it");
    assert_eq!(serde_json::to_string(&unchanged).expect("json"), second);
    for (root, file, shown) in [(&roots[1], later, 3), (&roots[2], &on_two[0], 2)] {
        let denied =
            swapped_page(root.path(), file, shown).expect_err("the page shows the swapped file");
        assert_eq!(denied.code, ErrorCode::SymlinkEscape, "{}", denied.message);
        assert!(
            denied.message.contains("denied by the active path policy"),
            "{}",
            denied.message
        );
    }
}

/// A key file the stored redaction scanned, rewritten in place under its
/// stored size and time after the page took the stored scan, restarts a
/// page that does not even show it: the replay reads the key file again and
/// binds it to the stored digest, as a fresh redaction does.
#[test]
fn a_key_file_rewritten_under_its_stamp_restarts_a_replayed_page() {
    let root = tempfile::tempdir().expect("fixture directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["needle"]}),
        None,
    );
    let snapshot = search_in(&request, &policy, &security, Some(6_000))
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let page_two = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    assert!(
        !page_paths(&page_two, &policy, &security)
            .iter()
            .any(|path| path == "zkey.ts")
    );
    let key = root.path().join("zkey.ts");
    let text = fs::read_to_string(&key).expect("key fixture");
    assert!(
        manifest::get(&snapshot, &policy.identity())
            .is_some_and(|stored| stored.redaction.is_some())
    );
    let page = stored_page_after(&page_two, &policy, &security, Some(6_000), || {
        rewrite_keeping_stamp(&key, &text.replace("export const pem", "export const pex"));
    });
    assert_eq!(
        page.expect_err("changed key file").code,
        ErrorCode::StaleSnapshot
    );
}

/// The same rewrite made before a continuation takes the stored scan
/// restarts it too. On Unix it moved the key file's change time, so the
/// walk's proof drops the stored scan before any replay.
#[test]
fn a_key_file_rewritten_before_a_continuation_restarts_it() {
    let root = tempfile::tempdir().expect("fixture directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["needle"]}),
        None,
    );
    let run = |query: &LocalSearchQuery| search_in(query, &policy, &security, Some(6_000));
    let snapshot = run(&request)
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let page_two = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    assert!(manifest::get(&snapshot, &policy.identity()).is_some());
    let key = root.path().join("zkey.ts");
    let text = fs::read_to_string(&key).expect("key fixture");
    rewrite_keeping_stamp(&key, &text.replace("export const pem", "export const pex"));
    if cfg!(unix) {
        assert!(manifest::get(&snapshot, &policy.identity()).is_none());
    }
    assert_eq!(
        run(&page_two).expect_err("changed key file").code,
        ErrorCode::StaleSnapshot
    );
}
