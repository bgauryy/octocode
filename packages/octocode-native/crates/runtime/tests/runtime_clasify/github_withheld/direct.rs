//! A read without classification sends nothing to a provider and returns the
//! same row, yet what it fetched of a credential-named file stays in this
//! process: nothing of it on disk, and its pages still walk losslessly.

use super::{MARKER, RAW_FILE, SHA, files_holding, github, opted_in};
use crate::support::{self, Run as _, Workspace};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn read(file: &str) -> Value {
    json!({"owner":"octo","repo":"demo","path":file,"ref":SHA})
}

#[tokio::test]
async fn a_direct_read_of_a_credential_file_leaves_no_copy_and_the_same_row() {
    let github = github().await;
    let settings = opted_in(&github);

    // The control: any other name is kept, so this scan does see a copy.
    let kept = Workspace::new();
    let runtime = kept.runtime(&settings);
    let plain = support::run(&runtime, "ghGetFileContent", read("docs/notes.txt")).await;
    runtime.close().await;
    assert!(
        files_holding(&kept.home, MARKER)
            .iter()
            .any(|file| file.contains("provider-answer")),
        "the control must leave a copy"
    );

    // A sensitive directory opens to no setting: refused before any read.
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings);
    let row = support::run(&runtime, "ghGetFileContent", read("a/.ssh/config")).await;
    runtime.close().await;
    assert_eq!(
        row.data()["errorCode"],
        "pathPolicyDenied",
        "{}",
        row.data()
    );
    assert_eq!(files_holding(&workspace.home, MARKER), Vec::<String>::new());

    for file in [".env", ".Env.Prod", "KEY.PEM", "./.env"] {
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&settings);
        let row = support::run(&runtime, "ghGetFileContent", read(file)).await;
        assert_eq!(row.status(), "success", "{file}: {}", row.data());
        for field in ["content", "commitSha", "totalLines"] {
            assert_eq!(
                row.data()[field],
                plain.data()[field],
                "{file}: {field} is unchanged"
            );
        }
        assert!(row.data()["content"].to_string().contains(MARKER));
        runtime.close().await;
        assert_eq!(
            files_holding(&workspace.home, MARKER),
            Vec::<String>::new(),
            "{file}: a copy stayed on disk"
        );
    }
}

/// A file too large for one page: the same runtime, and a new runtime per page
/// (each CLI call is its own), both reach every line exactly once.
#[tokio::test]
async fn a_walk_over_a_credential_file_stays_lossless_with_nothing_on_disk() {
    let github: MockServer = github().await;
    let body = (1..=3000)
        .map(|n| format!("line {n:04} {MARKER}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path_regex("/contents/\\.env$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_raw(body, RAW_FILE),
        )
        .with_priority(1)
        .mount(&github)
        .await;
    let settings = opted_in(&github);
    for restart_each_page in [false, true] {
        let workspace = Workspace::new();
        let mut runtime = workspace.runtime(&settings);
        let mut input = json!({"queries":[read(".env")]});
        let (mut lines, mut pages, mut refused) = (Vec::new(), 0, false);
        loop {
            pages += 1;
            assert!(pages < 200, "the walk must end");
            let result = runtime
                .run("walk".into(), "ghGetFileContent".into(), input.clone())
                .await;
            // The opt-in keeps saved work in memory: a new runtime refuses
            // the cursor honestly rather than resuming from disk.
            if restart_each_page && pages > 1 {
                let error = result.expect_err("saved work ends with its runtime");
                assert!(
                    format!("{error:?}").contains("ResponseSnapshotExpired"),
                    "{error:?}"
                );
                refused = true;
                break;
            }
            let out = result.expect("page").structured_content;
            let data = &out["results"][0]["data"];
            assert_ne!(out["results"][0]["status"], "error", "{out}");
            let content = data["content"].as_str().expect("content");
            lines.extend(content.lines().map(str::to_owned));
            let next = crate::support::next_input(&out);
            let Some(next) = next else {
                break;
            };
            input = next;
            if restart_each_page {
                runtime.close().await;
                runtime = workspace.runtime(&settings);
            }
        }
        runtime.close().await;
        assert!(pages > 1, "the fixture must take several pages");
        assert_eq!(refused, restart_each_page);
        if refused {
            assert_eq!(files_holding(&workspace.home, MARKER), Vec::<String>::new());
            continue;
        }
        let expected = (1..=3000)
            .map(|n| format!("{n}\tline {n:04} {MARKER}"))
            .collect::<Vec<_>>();
        assert_eq!(lines, expected, "restart_each_page={restart_each_page}");
        assert_eq!(
            files_holding(&workspace.home, MARKER),
            Vec::<String>::new(),
            "restart_each_page={restart_each_page}: a copy stayed on disk"
        );
    }
}

const BLOB: &str = "2222222222222222222222222222222222222222";

/// A repository whose files all sit past the contents API's limit, by either
/// answer GitHub gives for one: their bytes are only at `/git/blobs/<sha>`,
/// a request that names no file.
async fn past_the_contents_limit(too_large: bool, body: &str) -> MockServer {
    let github = github().await;
    let answer = if too_large {
        ResponseTemplate::new(413).set_body_json(json!({"message":"too large"}))
    } else {
        ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"none","content":"","size":2_000_000,"sha":BLOB
        }))
    };
    Mock::given(method("GET"))
        .and(path_regex("/contents/(\\.env|notes\\.txt|sub%2F\\.env)$"))
        .respond_with(answer)
        .with_priority(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/contents/(sub)?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":".env","type":"file","sha":BLOB},
            {"name":"notes.txt","type":"file","sha":BLOB},
        ])))
        .with_priority(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/octo/demo/git/blobs/{BLOB}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.to_owned(), RAW_FILE))
        .mount(&github)
        .await;
    github
}

/// Every line a walk of `file` reads, and its first row.
async fn walked(
    workspace: &Workspace,
    settings: &[(&str, String)],
    file: &str,
) -> (Vec<String>, Value) {
    let runtime = workspace.runtime(settings);
    let mut input = json!({"queries":[read(file)]});
    let (mut lines, mut first, mut pages) = (Vec::new(), None, 0);
    loop {
        pages += 1;
        assert!(pages < 200, "the walk must end");
        let out = runtime
            .run("walk".into(), "ghGetFileContent".into(), input.clone())
            .await
            .expect("page")
            .structured_content;
        assert_ne!(out["results"][0]["status"], "error", "{file}: {out}");
        let data = out["results"][0]["data"].clone();
        lines.extend(
            data["content"]
                .as_str()
                .expect("content")
                .lines()
                .map(str::to_owned),
        );
        let next = crate::support::next_input(&out);
        first.get_or_insert(data);
        let Some(next) = next else {
            break;
        };
        input = next;
    }
    runtime.close().await;
    assert!(pages > 1, "the fixture must take several pages");
    (lines, first.expect("a page"))
}

/// The blob's request names no file, so only the caller can say which file it
/// holds: the bytes of a credential-named one stay in this process, the row and
/// its walk unchanged.
#[tokio::test]
async fn a_blob_read_of_a_credential_file_leaves_no_copy_and_the_same_row() {
    let body = (1..=3000)
        .map(|n| format!("line {n:04} {MARKER}\n"))
        .collect::<String>();
    for too_large in [true, false] {
        let github = past_the_contents_limit(too_large, &body).await;
        let settings = opted_in(&github);

        // The control: any other name is kept, so this scan does see a copy.
        let kept = Workspace::new();
        let (plain, row) = walked(&kept, &settings, "notes.txt").await;
        assert!(
            files_holding(&kept.home, MARKER)
                .iter()
                .any(|file| file.contains("provider-answer")),
            "too_large={too_large}: the control must leave a copy"
        );

        for file in [".env", "sub/.env"] {
            let workspace = Workspace::new();
            let (lines, first) = walked(&workspace, &settings, file).await;
            assert_eq!(lines, plain, "{file}: every line, once");
            for field in ["content", "commitSha", "totalLines"] {
                assert_eq!(first[field], row[field], "{file}: {field} is unchanged");
            }
            assert_eq!(
                files_holding(&workspace.home, MARKER),
                Vec::<String>::new(),
                "too_large={too_large}, {file}: a copy stayed on disk"
            );
        }
    }
}
