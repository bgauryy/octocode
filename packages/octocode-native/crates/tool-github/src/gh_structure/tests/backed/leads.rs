//! Materialized pages and identities for caller-selected reads.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
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
        json_fixture(
            r#"[
                {
                    "name": "img.png",
                    "path": "img.png",
                    "type": "file",
                    "size": 4
                },
                {
                    "name": "ok.rs",
                    "path": "ok.rs",
                    "type": "file",
                    "size": 4
                }
            ]"#,
        ),
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
    let home = octocode_runtime_base::test_support::Fixture::new();
    let out = run_at(
        &server,
        json!({"operation":"tree", "owner":"a","repo":"b","ref":sha,"materialize":true}),
        &home.0,
    )
    .await
    .expect("materialize continues past binary files");
    let local = out.data["location"]["path"].as_str().expect("location");
    assert!(std::path::Path::new(local).join("ok.rs").exists());
    // `location.skipped` names the file; a skipped binary is not a gap in
    // what local text tools can read, so the checkout is complete.
    assert_eq!(
        out.data["location"]["skipped"],
        json_fixture(r#"["img.png"]"#),
        "{}",
        out.data
    );
    assert_eq!(
        out.data["location"]["coverage"]["state"], "complete",
        "{}",
        out.data
    );
    assert!(out.data.get("isPartial").is_none(), "{}", out.data);
    assert!(out.data.get("hints").is_none(), "{}", out.data);
    assert!(
        out.data["warnings"][0]
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
        .chain(std::iter::once(json_fixture(
            r#"{"path": "deep", "type": "tree", "sha": "0"}"#,
        )))
        .chain(std::iter::once(json_fixture(
            r#"{"path": "deep/er", "type": "tree", "sha": "0"}"#,
        )))
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
    let home = octocode_runtime_base::test_support::Fixture::new();
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
    let local = std::path::Path::new(out.data["location"]["path"].as_str().expect("path"));
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
        json_fixture(
            r#"[
                {"name": "a.rs", "path": "a.rs", "type": "file", "size": 8},
                {"name": "sub", "path": "sub", "type": "dir"}
            ]"#,
        ),
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
    let home = octocode_runtime_base::test_support::Fixture::new();
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
    use crate::gh_structure::materialize::{Snapshot, materialize_tree};
    use crate::gh_structure::{EntryKind, TreeEntry};
    // Reads run as many ahead as the request budget allows: three waves of
    // files, the third past the call's three-quarters stop. The margins are seconds, so a loaded machine's
    // scheduling delay cannot move a read past the deadline.
    const READ: std::time::Duration = std::time::Duration::from_millis(1_200);
    const CALL: std::time::Duration = std::time::Duration::from_secs(4);
    let server = MockServer::builder().start().await;
    let provider = octocode_tools_shared::gh_shared::test_support::one_shot_provider(&server);
    let files = 3 * provider.transport.read_concurrency();
    let entries = (0..files)
        .map(|n| TreeEntry {
            path: format!("f{n}.rs"),
            kind: EntryKind::File,
            size: Some(8),
            oid: None,
        })
        .collect::<Vec<_>>();
    for n in 0..files {
        wiremock::Mock::given(wiremock::matchers::path(format!(
            "/api/v3/repos/a/b/contents/f{n}.rs"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(
                    b"fn u(){}".to_vec(),
                    "application/vnd.github.raw; charset=utf-8",
                )
                .set_delay(READ),
        )
        .mount(&server)
        .await;
    }
    let home = octocode_runtime_base::test_support::Fixture::new();
    let snapshot = Snapshot {
        owner: "a",
        repo: "b",
        commit: SHA,
        root: &home.0,
        blobs: None,
    };
    let mut offset = 0;
    let mut calls = 0;
    let local = loop {
        calls += 1;
        let context = octocode_github::RequestContext::with_timeout(CALL, 1024 * 1024);
        let outcome = materialize_tree(&provider, &snapshot, &entries, offset, "", &context)
            .await
            .expect("page")
            .expect("entries left");
        let local = outcome.location["path"].as_str().expect("path").to_owned();
        match outcome.next_offset {
            Some(next) => {
                assert!(next > offset, "{next} after {offset}: the walk progresses");
                offset = next;
            }
            None => break local,
        }
    };
    assert!(calls > 1, "the deadline ended the first call");
    for n in 0..files {
        assert!(
            std::path::Path::new(&local)
                .join(format!("f{n}.rs"))
                .exists(),
            "f{n}"
        );
    }
    let reads = server.received_requests().await.unwrap().len();
    assert!(reads >= files, "{reads}");
}

/// A first read that arrives past the call's three-quarters stop is still
/// written: a call that wrote nothing would resume at the same entry forever.
/// The read's delay sits between the stop and the deadline, so the order is
/// the same on every machine.
#[tokio::test]
async fn a_first_read_past_the_stop_is_written_so_the_walk_progresses() {
    use crate::gh_structure::materialize::{Snapshot, materialize_tree};
    use crate::gh_structure::{EntryKind, TreeEntry};
    const CALL: std::time::Duration = std::time::Duration::from_secs(4);
    let server = MockServer::builder().start().await;
    let entries = (0..2)
        .map(|n| TreeEntry {
            path: format!("f{n}.rs"),
            kind: EntryKind::File,
            size: Some(8),
            oid: None,
        })
        .collect::<Vec<_>>();
    for n in 0..2 {
        wiremock::Mock::given(wiremock::matchers::path(format!(
            "/api/v3/repos/a/b/contents/f{n}.rs"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(
                    b"fn u(){}".to_vec(),
                    "application/vnd.github.raw; charset=utf-8",
                )
                .set_delay(CALL.mul_f64(0.85)),
        )
        .mount(&server)
        .await;
    }
    let provider = octocode_tools_shared::gh_shared::test_support::one_shot_provider(&server);
    let home = octocode_runtime_base::test_support::Fixture::new();
    let snapshot = Snapshot {
        owner: "a",
        repo: "b",
        commit: SHA,
        root: &home.0,
        blobs: None,
    };
    let context = octocode_github::RequestContext::with_timeout(CALL, 1024 * 1024);
    let outcome = materialize_tree(&provider, &snapshot, &entries, 0, "", &context)
        .await
        .expect("page")
        .expect("entries left");
    assert!(
        outcome.next_offset.is_none_or(|next| next > 0),
        "the call wrote nothing and resumes where it began: {:?}",
        outcome.next_offset
    );
}

/// A listing keeps every selected file and its commit identity, allowing
/// the caller to choose an outline read without generated read advice.
#[tokio::test]
async fn a_listing_exposes_the_identity_for_an_explicit_outline_read() {
    let server = MockServer::builder().start().await;
    let files = (0..12)
            .map(|n| json!({"name":format!("m{n}.py"),"path":format!("pkg/m{n}.py"),"type":"file","size":4}))
            .chain(std::iter::once(json_fixture(r#"{
                "name": "__init__.py",
                "path": "pkg/__init__.py",
                "type": "file",
                "size": 4
            }"#)))
            .collect::<Vec<_>>();
    mount_listing(&server, "pkg", json!(files)).await;
    let out = run(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"pkg"}),
    )
    .await
    .expect("listing");
    assert_eq!(out.data["files"][0]["dir"], "pkg", "{}", out.data);
    assert!(out.data["files"].to_string().contains("__init__.py (4)"));
    assert_eq!(out.data["commitSha"], SHA);
    assert!(out.data.get("related").is_none(), "{}", out.data);
    let read = json!({"owner":out.data["owner"],"repo":out.data["repo"],"path":"pkg/__init__.py","ref":out.data["commitSha"],"view":"symbols"});
    octocode_runtime_base::contracts::validate_query("ghGetFileContent", read)
        .expect("listed identity forms a valid read");
}

/// A materialized listing identifies the directory and exact source on disk.
#[tokio::test]
async fn materialize_exposes_the_local_copy_of_the_listed_directory() {
    let server = MockServer::builder().start().await;
    mount_listing(
        &server,
        "src",
        json_fixture(
            r#"[
                {
                    "name": "lib.rs",
                    "path": "src/lib.rs",
                    "type": "file",
                    "size": 8
                }
            ]"#,
        ),
    )
    .await;
    mount_content(&server, "src/lib.rs", "fn x(){}").await;
    let home = octocode_runtime_base::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize");
    let local = out.data["location"]["path"]
        .as_str()
        .expect("location")
        .to_owned();
    assert!(local.ends_with("/src"), "{}", out.data);
    assert!(std::path::Path::new(&local).join("lib.rs").exists());
    assert!(
        out.data.pointer("/next/exploreClone").is_none(),
        "{}",
        out.data
    );
    assert!(out.data.get("related").is_none(), "{}", out.data);
    let path = std::path::Path::new(&local).join("lib.rs");
    assert_eq!(
        std::fs::read_to_string(&path).expect("materialized source"),
        "fn x(){}"
    );
    octocode_runtime_base::contracts::validate_query(
        "localFetch",
        json!({"path":path,"view":"symbols"}),
    )
    .expect("local identity forms a valid read");
}

/// Materialization distinguishes the files it wrote from oversized files
/// it skipped, preserving partial coverage without optional read advice.
#[tokio::test]
async fn materialize_reports_files_it_did_not_write() {
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
    let home = octocode_runtime_base::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize");
    assert_eq!(
        out.data["location"]["skipped"],
        json_fixture(r#"["src/lib.rs"]"#),
        "{}",
        out.data
    );
    let local = std::path::Path::new(out.data["location"]["path"].as_str().expect("local path"));
    assert!(!local.join("lib.rs").exists(), "{}", out.data);
    assert_eq!(
        std::fs::read_to_string(local.join("util.rs")).expect("written file"),
        "fn u(){}"
    );
    assert_eq!(
        out.data["location"]["coverage"]["state"], "partial",
        "{}",
        out.data
    );
    assert!(out.data.get("related").is_none(), "{}", out.data);
}
