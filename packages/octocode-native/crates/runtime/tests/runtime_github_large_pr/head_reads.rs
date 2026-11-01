//! Head-text reads: match context past or clipped by a hunk, large blobs and merge-side code picks.

use super::{SHA, mount_file_batches, mount_pr, rest_file, run, run_status};
use crate::support;
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// cli/cli#13541: the first patch window holds only docs, a test, and a
/// test too large for a patch, so its code file sits in a later window.
/// `readAtMerge` still names the most-changed code file of the whole PR
/// file list, not only of the window shown.
#[tokio::test]
async fn read_at_merge_picks_the_code_file_of_the_whole_pr_not_the_first_window() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 5).await;
    let test_patch = format!(
        "@@ -0,0 +1,3000 @@\n{}",
        (1..=3000)
            .map(|line| format!("+\tassert(t, line{line}, expected{line})"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let added = |name: &str, patch: Option<&str>, additions: u64| {
        let mut file = rest_file(name, patch, additions, 0);
        file["status"] = json!("added");
        file
    };
    let files = vec![
        added(
            "acceptance/README.md",
            Some("@@ -0,0 +1 @@\n+Run the suite"),
            1,
        ),
        added("acceptance/acceptance_test.go", Some(&test_patch), 3000),
        added("pkg/client/client_test.go", None, 3842),
        added(
            "pkg/client/client.go",
            Some("@@ -0,0 +1,3 @@\n+package client\n+func Alpha() {}\n+func Beta() {}"),
            3,
        ),
        added("pkg/client/types.go", Some("@@ -0,0 +1 @@\n+type T int"), 1),
    ];
    mount_file_batches(&server, vec![files], Duration::ZERO).await;
    use base64::Engine as _;
    let merge = "fedcba9876543210fedcba9876543210fedcba98";
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/pkg%2Fclient%2Fclient.go"))
        .and(query_param("ref",merge))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","path":"pkg/client/client.go","content":base64::engine::general_purpose::STANDARD.encode("package client\nfunc Alpha() {}\nfunc Beta() {}\n")})))
        .expect(1).mount(&server).await;
    let workspace = crate::support::Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let result = crate::support::run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"sections":["patches"]}),
    )
    .await;
    let data = result.data();
    let shown = data["pullRequests"][0]["files"]
        .as_array()
        .expect("patch rows")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(!shown.contains(&"pkg/client/client.go"), "{data}");
    let read = support::checks::related_call(data, "ghGetFileContent", |input| {
        input["queries"][0]["ref"] == merge && input["queries"][0]["path"] == "pkg/client/client.go"
    });
    assert_eq!(read["kind"], "related");
    let source = crate::support::run(&runtime, "ghGetFileContent", read["input"].clone()).await;
    assert_eq!(source.status(), "success", "{}", source.data());
    assert!(source.data().to_string().contains("func Alpha() {}"));
    server.verify().await;
    runtime.close().await;
}

/// A hunk with GitHub's 3 context lines around a hit at line 53 of a
/// 100-line file, and that file at the head.
fn hunk_at_53() -> (String, String) {
    let patch = "@@ -50,7 +50,7 @@ fn f\n line 50\n line 51\n line 52\n-old 53\n+new needle 53\n line 54\n line 55\n line 56\n".to_owned();
    let head = (1..=100)
        .map(|n| {
            if n == 53 {
                "new needle 53\n".to_owned()
            } else {
                format!("line {n}\n")
            }
        })
        .collect();
    (patch, head)
}

/// contextLines past GitHub's hunk context: the read widens the run from
/// the file at the PR head (`headSha`), so the default 10 lines are not
/// silently cut to the hunk's 3.
#[tokio::test]
async fn match_context_past_the_hunk_reads_the_head_text() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    Mock::given(method("GET"))
        // The shared file read names the path as one segment, as
        // ghGetFileContent does.
        .and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","path":"src/a.rs",
            "content": base64::engine::general_purpose::STANDARD.encode(&head)
        })))
        .mount(&server)
        .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": ["needle"], "contextLines": {"before":10,"after":10}}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    let text = file["patch"].as_str().expect("patch");
    assert!(
        text.starts_with("@@ -44,20 +44,20 @@ fn f\n44\t line 44\n"),
        "{text}"
    );
    assert!(text.ends_with("63\t line 63\n"), "{text}");
    assert!(file.get("contextClipped").is_none(), "{data}");
    assert!(
        data.get("next")
            .is_none_or(|next| next.get("expandContext").is_none()),
        "{data}"
    );
}

/// Every file on the page whose hits need head text gets it, however many
/// there are: none is left clipped behind a per-call count.
#[tokio::test]
async fn every_file_on_the_page_reads_its_head_text() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    let count = 12;
    mount_pr(&server, count).await;
    let (patch, head) = hunk_at_53();
    let names = (0..count)
        .map(|n| format!("src/f{n:02}.rs"))
        .collect::<Vec<_>>();
    // Distinct patches, so no file is shown as another's duplicate.
    let own = |text: &str, name: &str| text.replace("needle 53", &format!("needle 53 {name}"));
    mount_file_batches(
        &server,
        vec![
            names
                .iter()
                .map(|name| rest_file(name, Some(&own(&patch, name)), 1, 1))
                .collect(),
        ],
        Duration::ZERO,
    )
    .await;
    for name in &names {
        let head = own(&head, name);
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v3/repos/a/b/contents/{}",
                name.replace('/', "%2F")
            )))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","path":name,
                "content": base64::engine::general_purpose::STANDARD.encode(&head)
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": ["needle"], "contextLines": {"before":10,"after":10}}),
    )
    .await;
    let files = data["pullRequests"][0]["files"].as_array().expect("files");
    assert_eq!(files.len(), count, "{data}");
    for file in files {
        assert!(file.get("contextClipped").is_none(), "{file}");
        assert!(
            file["patch"]
                .as_str()
                .is_some_and(|text| text.ends_with("63\t line 63\n")),
            "{file}"
        );
    }
    assert!(
        data.get("next")
            .is_none_or(|next| next.get("expandContext").is_none()),
        "{data}"
    );
    server.verify().await;
}

/// A head file past the Contents API's 1 MB body (`encoding:"none"`, no
/// content) is read through its blob, as ghGetFileContent reads it: the
/// widened context is not silently lost to an empty body.
#[tokio::test]
async fn a_head_file_past_one_megabyte_is_read_through_its_blob() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    let blob = "b".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"none","content":"","path":"src/a.rs",
            "size": 2_000_000, "sha": blob
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/vnd.github.raw")
                .set_body_string(head),
        )
        .expect(1)
        .mount(&server)
        .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": ["needle"], "contextLines": {"before":10,"after":10}}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    let text = file["patch"].as_str().expect("patch");
    assert!(
        text.starts_with("@@ -44,20 +44,20 @@ fn f\n44\t line 44\n"),
        "{text}"
    );
    assert!(file.get("contextClipped").is_none(), "{data}");
    server.verify().await;
}

/// A later file page widens its own hits: the head reads name the files
/// this page shows, never the first page's again.
#[tokio::test]
async fn a_scoped_match_reads_only_its_selected_head_file() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some(&patch), 1, 1),
            rest_file("src/b.rs", Some(&patch), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    for (file, reads) in [("a", 0), ("b", 1)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{file}.rs")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","path":format!("src/{file}.rs"),
                "content": base64::engine::general_purpose::STANDARD.encode(&head)
            })))
            .expect(reads)
            .mount(&server)
            .await;
    }
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": ["needle"], "contextLines": {"before":10,"after":10},
            "include":["src/b.rs"]}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["path"], "src/b.rs", "{data}");
    assert!(file.get("contextClipped").is_none(), "{data}");
    server.verify().await;
}

/// Without the head text (the read failed), the clip is explicit: the file
/// is flagged and `next.expandContext` reads the wanted lines at headSha.
#[tokio::test]
async fn match_context_clipped_by_the_hunk_is_flagged_with_a_head_read() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 1).await;
    let (patch, _) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    let (_, data) = run_status(
        &server,
        json!({"sections":["patches"], "matchString": ["needle"], "contextLines": {"before":10,"after":10}}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["contextClipped"], true, "{data}");
    let read = &data["next"]["expandContext"];
    assert_eq!(read["tool"], "ghGetFileContent", "{data}");
    assert!(read["input"]["cursor"].is_string(), "{data}");
    assert!(
        data["partialReasons"]
            .to_string()
            .contains("contextClipped"),
        "{data}"
    );
}

/// A merged pull request's `matchString` read offers the hit
/// file at the PR head (`readAtCommit` at `headSha`, where the gutter
/// numbers hold) and no `readAtMerge` (a targeted answer already).
#[tokio::test]
async fn history_b_merged_match_string_read_offers_the_source_side_only() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file(
                "src/hit.rs",
                Some("@@ -1,3 +1,3 @@ fn f\n a\n-old needle\n+new needle\n b"),
                1,
                1,
            ),
            rest_file("src/other.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"matchString": ["needle"], "contextLines": {"before":1,"after":1}}),
    )
    .await;
    let at_commit = support::checks::related_call(&data, "ghGetFileContent", |input| {
        input["queries"][0]["ref"] == SHA && input["queries"][0]["path"] == "src/hit.rs"
    });
    assert_eq!(at_commit["kind"], "related");
    assert!(
        support::checks::find_related_call(&data, "ghGetFileContent", |input| {
            input["queries"][0]["ref"] == "fedcba9876543210fedcba9876543210fedcba98"
        })
        .is_none(),
        "{data}"
    );
    // The hit row counts the hits it shows.
    assert_eq!(
        data["pullRequests"][0]["files"][0]["matchCount"], 2,
        "{data}"
    );
}
