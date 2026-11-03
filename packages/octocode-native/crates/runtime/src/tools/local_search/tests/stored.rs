//! Stored scans and continuation pages: a later page is cut from the
//! scan its first page stored, equals the page a rescan cuts, and restarts
//! when the walk, a shown file or the snapshot no longer holds.
use super::*;
use octocode_contract_types::{ErrorCode, LocalSearchQuery};

mod swaps;

/// Swap `path` for a symlink to a copy outside the allowed root with the
/// same bytes and modification time: size, time and digest all match the
/// stored scan, and only the read policy sees the change.
#[cfg(unix)]
fn swap_for_outside_copy(path: &std::path::Path, outside: &std::path::Path) {
    let copy = outside.join(path.file_name().expect("file name"));
    fs::copy(path, &copy).expect("copy outside");
    let modified = fs::metadata(path)
        .and_then(|m| m.modified())
        .expect("mtime");
    fs::File::options()
        .write(true)
        .open(&copy)
        .and_then(|file| file.set_modified(modified))
        .expect("restore mtime");
    fs::remove_file(path).expect("remove");
    std::os::unix::fs::symlink(&copy, path).expect("symlink");
}

/// The files a page of the stored scan shows (`path` list).
fn page_paths(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> Vec<String> {
    search_in(request, policy, security, Some(6_000))
        .expect("page")
        .files
        .into_iter()
        .map(|file| file.path)
        .collect()
}

/// Thirty TS files of needle hits (every third also holds a token on a hit
/// line), a key file whose hit sits inside a private-key block, and an
/// innocent base64 hit: enough rows for several pages, with values that
/// redaction replaces and files whose key-block scans read their bytes.
fn paged_fixture(root: &std::path::Path) {
    for file in 0..30 {
        let mut body = (0..15)
            .map(|n| {
                format!(
                    "export function compute_{file}_{n}(value: number): number {{\n  const needle_{n} = value * {n};\n  return needle_{n} + {file};\n}}\n"
                )
            })
            .collect::<String>();
        if file % 3 == 0 {
            body.push_str(&format!(
                "const needle_token = \"ghp_{file:02}ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567\";\n"
            ));
        }
        fs::write(root.join(format!("m{file:02}.ts")), body).expect("fixture");
    }
    fs::write(
        root.join("zkey.ts"),
        "export const pem = `\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAneedleKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n`;\n",
    )
    .expect("key fixture");
    fs::write(
        root.join("zbase64.ts"),
        "export const blob = `\nneedleAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n`;\n",
    )
    .expect("base64 fixture");
}

#[test]
fn same_size_rewrites_invalidate_content_and_count_continuations() {
    for view in ["content", "files", "countMatches"] {
        let root = tempfile::tempdir().expect("fixture");
        paged_fixture(root.path());
        settle();
        let (policy, security) = policy_for(root.path());
        let q = ls_query(
            serde_json::json!({"path":root.path(),"matchString":["needle"],"view":view,"sort":{"by":"path","order":"asc"}}),
            None,
        );
        let first = search_in(&q, &policy, &security, Some(1500)).expect("first page");
        let snapshot = first.source_snapshot.expect("snapshot");
        let continued = ls_query(serde_json::json!({"snapshot":snapshot,"page":2}), Some(&q));
        let path = root.path().join("m00.ts");
        let source = fs::read_to_string(&path).expect("source");
        rewrite_keeping_stamp(&path, &source.replace("needle", "nEEdle"));
        let error = search_in(&continued, &policy, &security, Some(1500))
            .expect_err("a rewrite must not mix evidence");
        assert_eq!(error.code, ErrorCode::StaleSnapshot, "{view}: {error:?}");
    }
}

/// A scan is deflated only when a page issues a cursor that keeps it: a
/// one-page answer compresses nothing, a paged one compresses its scan once
/// and its continuation is served from it.
#[test]
fn only_a_page_that_issues_a_cursor_deflates_its_scan() {
    let single = tempfile::tempdir().expect("fixture");
    for file in 0..3 {
        fs::write(
            single.path().join(format!("s{file}.ts")),
            format!("export const needle_{file} = {file};\n"),
        )
        .expect("fixture");
    }
    settle();
    let (policy, security) = policy_for(single.path());
    let q = ls_query(
        serde_json::json!({"path":single.path(),"matchString":["needle"]}),
        None,
    );
    let before = super::super::packed::deflated_on_this_thread();
    let page = search_in(&q, &policy, &security, None).expect("single page");
    assert_eq!(page.files.len(), 3);
    let continues = |next: &Option<serde_json::Value>| {
        next.as_ref()
            .is_some_and(|next| next.get("nextPage").is_some())
    };
    assert!(!continues(&page.next), "one page: {:?}", page.next);
    assert_eq!(
        super::super::packed::deflated_on_this_thread() - before,
        0,
        "a one-page answer deflates nothing"
    );

    let paged = tempfile::tempdir().expect("fixture");
    paged_fixture(paged.path());
    settle();
    let (policy, security) = policy_for(paged.path());
    let q = ls_query(
        serde_json::json!({"path":paged.path(),"matchString":["needle"]}),
        None,
    );
    let before = super::super::packed::deflated_on_this_thread();
    let first = search_in(&q, &policy, &security, Some(2500)).expect("first page");
    assert!(continues(&first.next), "paged: {:?}", first.next);
    let deflated = super::super::packed::deflated_on_this_thread() - before;
    assert!(deflated > 0, "a page that issues a cursor keeps its scan");
    let continued = ls_query(
        serde_json::json!({"snapshot":first.source_snapshot.expect("snapshot"),"page":2}),
        Some(&q),
    );
    let scan =
        super::super::executor::scan_query(&continued, &policy, &NeverCancel, (Some(2500), None))
            .expect("scan");
    assert!(scan.digests.is_some(), "page 2 is cut from the stored scan");
    assert_eq!(
        super::super::packed::deflated_on_this_thread() - before,
        deflated,
        "a continuation deflates nothing"
    );
}

#[test]
fn stored_snapshot_is_bound_to_search_meaning_and_window() {
    let root = tempfile::tempdir().expect("fixture");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let q = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        None,
    );
    let first = search_in(&q, &policy, &security, Some(2500)).expect("first");
    let continued = ls_query(
        serde_json::json!({"snapshot":first.source_snapshot.expect("snapshot"),"page":2}),
        Some(&q),
    );
    for change in [
        serde_json::json!({"matchString":["compute"]}),
        serde_json::json!({"caseMode":"sensitive"}),
        serde_json::json!({"include":["*.rs"]}),
    ] {
        let changed = ls_query(change, Some(&continued));
        assert_eq!(
            search_in(&changed, &policy, &security, Some(2500))
                .expect_err("different meaning")
                .code,
            ErrorCode::StaleSnapshot
        );
    }
    assert_eq!(
        search_in(&continued, &policy, &security, Some(6000))
            .expect_err("different budget")
            .code,
        ErrorCode::StaleSnapshot
    );
    let before = search_in(&continued, &policy, &security, Some(2500)).expect("stable replay");
    let after = search_in(&continued, &policy, &security, Some(2500)).expect("repeat replay");
    assert_eq!(values(&before), values(&after));
}
