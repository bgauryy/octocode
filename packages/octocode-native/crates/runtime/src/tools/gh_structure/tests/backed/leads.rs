//! Materialized pages and the reads a listing leads to.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_github::mock_http::mount_json;
use serde_json::json;
use wiremock::MockServer;

#[tokio::test]
async fn materialize_skips_binary_files_with_a_warning() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"img.png","path":"img.png","type":"file","size":4},
            {"name":"ok.rs","path":"ok.rs","type":"file","size":4}
        ]),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/commits/{sha}"),
        200,
        json!({"sha":sha}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/img.png",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode([0u8, 1, 2, 3])
        }),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/ok.rs",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn x(){}")
        }),
    )
    .await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
        &server,
        json!({"operation":"tree", "owner":"a","repo":"b","ref":sha,"materialize":true}),
        &home.0,
    )
    .await
    .expect("materialize continues past binary files");
    let local = out.data["location"]["localPath"]
        .as_str()
        .expect("location");
    assert!(std::path::Path::new(local).join("ok.rs").exists());
    // `location.skipped` names the file; a skipped binary is not a gap in
    // what local text tools can read, so the checkout is complete.
    assert_eq!(
        out.data["location"]["skipped"],
        json!(["img.png"]),
        "{}",
        out.data
    );
    assert_eq!(
        out.data["location"]["coverage"]["state"], "complete",
        "{}",
        out.data
    );
    assert!(out.data.get("isPartial").is_none(), "{}", out.data);
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
    assert!(
        out.data["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("1 binary file")),
        "{}",
        out.data
    );
}

/// One call writes the whole selection: every level, every listing page, past
/// what the former per-page caps stopped at, and the listing's own pages stay
/// reachable without writing anything again.
#[tokio::test]
async fn one_materialize_writes_the_whole_selection_and_reports_complete() {
    let server = MockServer::builder().start().await;
    let tree = (0..70)
        .map(|n| json!({"path":format!("deep/er/f{n}.rs"),"type":"blob","size":8,"sha":"0"}))
        .chain(std::iter::once(
            json!({"path":"deep","type":"tree","sha":"0"}),
        ))
        .chain(std::iter::once(
            json!({"path":"deep/er","type":"tree","sha":"0"}),
        ))
        .collect::<Vec<_>>();
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha":SHA,"truncated":false,"tree":tree}),
    )
    .await;
    for n in 0..70 {
        mount_content(&server, &format!("deep/er/f{n}.rs"), "fn u(){}").await;
    }
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"materialize":true,"limit":10}),
        &home.0,
    )
    .await
    .expect("materialize");
    assert_eq!(
        out.data["location"]["coverage"]["state"], "complete",
        "{}",
        out.data
    );
    assert!(out.data.get("isPartial").is_none(), "{}", out.data);
    assert!(
        out.data.pointer("/next/continueMaterialize").is_none(),
        "{}",
        out.data
    );
    let local = std::path::Path::new(out.data["location"]["localPath"].as_str().expect("path"));
    for n in 0..70 {
        assert!(local.join(format!("deep/er/f{n}.rs")).exists(), "f{n}");
    }
    let next = &out.data["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(next["page"], 2, "{}", out.data);
    assert!(next.get("materialize").is_none(), "{next}");
}

/// A subtree the walk could not read leaves the checkout partial, whatever it
/// wrote of the rest.
#[tokio::test]
async fn a_materialize_of_a_walk_with_a_failed_subtree_is_not_complete() {
    let server = MockServer::builder().start().await;
    mount_listing(
        &server,
        "",
        json!([
            {"name":"a.rs","path":"a.rs","type":"file","size":8},
            {"name":"sub","path":"sub","type":"dir"}
        ]),
    )
    .await;
    mount_content(&server, "a.rs", "fn u(){}").await;
    wiremock::Mock::given(wiremock::matchers::path("/api/v3/repos/a/b/contents/sub"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::path_regex("/git/trees/"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"materialize":true}),
        &home.0,
    )
    .await
    .expect("materialize");
    assert_eq!(
        out.data["location"]["coverage"]["state"], "partial",
        "{}",
        out.data
    );
    assert_eq!(out.data["isPartial"], true, "{}", out.data);
}

/// Out of time, a materialize keeps what it wrote and resumes from the entry
/// it stopped at: no file twice, none lost, and no second listing of the tree.
#[tokio::test]
async fn a_materialize_out_of_time_resumes_without_gaps_or_a_second_tree() {
    use crate::tools::gh_structure::materialize::{Snapshot, materialize_tree};
    use crate::tools::gh_structure::{EntryKind, TreeEntry};
    let server = MockServer::builder().start().await;
    let entries = (0..24)
        .map(|n| TreeEntry {
            path: format!("f{n}.rs"),
            kind: EntryKind::File,
            size: Some(8),
        })
        .collect::<Vec<_>>();
    for n in 0..24 {
        wiremock::Mock::given(wiremock::matchers::path(format!(
            "/api/v3/repos/a/b/contents/f{n}.rs"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(
                    b"fn u(){}".to_vec(),
                    "application/vnd.github.raw; charset=utf-8",
                )
                .set_delay(std::time::Duration::from_millis(150)),
        )
        .mount(&server)
        .await;
    }
    let provider = crate::tools::gh_shared::test_support::one_shot_provider(&server);
    let home = crate::tools::test_support::Fixture::new();
    let snapshot = Snapshot {
        owner: "a",
        repo: "b",
        commit: SHA,
        root: &home.0,
    };
    let mut offset = 0;
    let mut calls = 0;
    let local = loop {
        calls += 1;
        let context = octocode_github::RequestContext::with_timeout(
            std::time::Duration::from_millis(500),
            1024 * 1024,
        );
        let outcome = materialize_tree(&provider, &snapshot, &entries, offset, "", &context)
            .await
            .expect("page")
            .expect("entries left");
        let local = outcome.location["localPath"]
            .as_str()
            .expect("path")
            .to_owned();
        match outcome.next_offset {
            Some(next) => {
                assert!(next > offset, "{next} after {offset}: the walk progresses");
                offset = next;
            }
            None => break local,
        }
    };
    assert!(calls > 1, "the deadline ended the first call");
    for n in 0..24 {
        assert!(
            std::path::Path::new(&local)
                .join(format!("f{n}.rs"))
                .exists(),
            "f{n}"
        );
    }
    let reads = server.received_requests().await.unwrap().len();
    assert!(reads >= 24, "{reads}");
}

/// A listing leads to the read a caller makes next: the package's
/// entry file (outlined) in a wide directory, at the listed commit.
#[tokio::test]
async fn a_listing_leads_to_an_outline_read_of_its_entry_file() {
    let server = MockServer::builder().start().await;
    let files = (0..12)
            .map(|n| json!({"name":format!("m{n}.py"),"path":format!("pkg/m{n}.py"),"type":"file","size":4}))
            .chain(std::iter::once(json!({"name":"__init__.py","path":"pkg/__init__.py","type":"file","size":4})))
            .collect::<Vec<_>>();
    mount_listing(&server, "pkg", json!(files)).await;
    let out = run(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"pkg"}),
    )
    .await
    .expect("listing");
    let read =
        crate::tools::test_support::checks::related_call(&out.data, "ghGetFileContent", |_| true);
    assert_eq!(read["tool"], "ghGetFileContent", "{}", out.data);
    let row = &read["input"]["queries"][0];
    assert_eq!(row["path"], "pkg/__init__.py", "{}", out.data);
    assert_eq!(row["ref"], SHA, "{}", out.data);
    assert_eq!(row["view"], "symbols", "{}", out.data);
}

/// A materialized listing points at the listed directory on disk.
#[tokio::test]
async fn materialize_leads_into_the_local_copy_of_the_listed_directory() {
    let server = MockServer::builder().start().await;
    mount_listing(
        &server,
        "src",
        json!([{"name":"lib.rs","path":"src/lib.rs","type":"file","size":8}]),
    )
    .await;
    mount_content(&server, "src/lib.rs", "fn x(){}").await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize");
    let local = out.data["location"]["localPath"]
        .as_str()
        .expect("location")
        .to_owned();
    assert!(local.ends_with("/src"), "{}", out.data);
    assert!(std::path::Path::new(&local).join("lib.rs").exists());
    // No lead re-lists the tree this response just returned; the lead reads
    // the best-ranked written file from the local copy.
    assert!(
        out.data.pointer("/next/exploreClone").is_none(),
        "{}",
        out.data
    );
    let read = crate::tools::test_support::checks::related_call(&out.data, "localFetch", |_| true);
    let row = &read["input"]["queries"][0];
    assert_eq!(
        row["path"],
        std::path::Path::new(&local)
            .join("lib.rs")
            .to_string_lossy()
            .as_ref(),
        "{}",
        out.data
    );
    assert_eq!(row["view"], "symbols", "{}", out.data);
}

/// The materialize lead names only a file this page wrote: a skipped
/// (oversized) file is never the lead, and a page that wrote no readable
/// file has none.
#[tokio::test]
async fn materialize_lead_skips_files_it_did_not_write() {
    let server = MockServer::builder().start().await;
    // Past what the call may read, by its listed size.
    let big_size = 1u64 << 40;
    mount_listing(
        &server,
        "src",
        json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file","size":big_size},
            {"name":"util.rs","path":"src/util.rs","type":"file","size":8}
        ]),
    )
    .await;
    mount_content(&server, "src/util.rs", "fn u(){}").await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize");
    assert_eq!(
        out.data["location"]["skipped"],
        json!(["src/lib.rs"]),
        "{}",
        out.data
    );
    let read = crate::tools::test_support::checks::related_call(&out.data, "localFetch", |_| true);
    assert!(
        read["input"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/src/util.rs")),
        "{}",
        out.data
    );
}
