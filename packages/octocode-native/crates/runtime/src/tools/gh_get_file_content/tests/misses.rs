//! Missed matches: empty reads, their leads and the fallback hint.
use super::*;

/// A matchString with no hit in an existing file is `matchNotFound` with
/// an empty `content` (the contract requires it, as in localFetch) and
/// its leads; it carries no prose beside them, no zero counters and no
/// echo of the request.
#[tokio::test]
async fn no_match_read_keeps_its_leads_without_empty_echo() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\ntwo\n")}),).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":["absent"]}),
    )
    .await
    .expect("read");
    let data = &read.data;
    assert_eq!(read.status, Some("empty"), "{data}");
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(data["content"], "", "{data}");
    assert_eq!(
        data["commitSha"], sha,
        "a pinned read retains its source: {data}"
    );
    crate::tools::test_support::validate_sealed(
        "ghGetFileContent",
        &serde_json::json!({"results":[{"index":0,"status":"empty","data":data}]}),
    )
    .expect("an empty match row satisfies the output contract");
    for junk in [
        "returnedLines",
        "returnedChars",
        "searchedFor",
        "pagination",
        "hints",
        "warnings",
    ] {
        assert!(data.get(junk).is_none(), "{junk}: {data}");
    }
    let lead = crate::tools::test_support::checks::related_call(data, "ghSearchCode", |_| true);
    assert_eq!(lead["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        lead["input"]["queries"][0]["keywords"],
        serde_json::json!(["absent"]),
        "{data}"
    );
    // Code search explores the default index; the separate history lead is pinned.
    assert!(lead["input"]["queries"][0].get("ref").is_none(), "{data}");
    // The text may live only on a branch or pull request: the file's
    // commits at the read's ref, and pull requests whose text names it
    // (approximate: their discussion, not their diffs).
    let commits =
        crate::tools::test_support::checks::related_call(data, "ghSearchHistory", |input| {
            input["queries"][0]["operation"] == "commit"
        });
    assert_eq!(commits["tool"], "ghSearchHistory", "{data}");
    assert_eq!(
        commits["input"]["queries"][0],
        serde_json::json!({"operation":"commit","owner":"a","repo":"b","pathPrefix":"a.txt","ref":sha}),
        "{data}"
    );
    let pulls =
        crate::tools::test_support::checks::related_call(data, "ghSearchHistory", |input| {
            input["queries"][0]["operation"] == "pullRequest"
        });
    assert_eq!(
        pulls["input"]["queries"][0],
        serde_json::json!({"operation":"pullRequest","owner":"a","repo":"b","keywords":["absent"]}),
        "{data}"
    );
    assert_eq!(pulls["kind"], "related", "{data}");
    // A miss that no case-insensitive read would select has no
    // ignoreCase lead.
    assert!(data["next"].get("ignoreCase").is_none(), "{data}");
    // The shared localFetch miss offers a localSearch of the file's
    // directory; it has no GitHub equivalent, so it must not be rewritten
    // into a ghGetFileContent read of that directory (an invalidInput
    // "is a directory" row when run verbatim).
    assert!(data["next"].get("findText").is_none(), "{data}");
    for (name, lead) in data["next"].as_object().into_iter().flatten() {
        if lead["tool"] == "ghGetFileContent" {
            assert_eq!(
                lead["input"]["queries"][0]["path"], "a.txt",
                "{name} reads another path: {data}"
            );
        }
    }

    // A case-sensitive miss that a case-insensitive read selects runs
    // the same read, ignoring case, at the read's commit; the count of
    // lines it selects is the fact beside the lead.
    let sensitive = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":["Two"]}),
    )
    .await
    .expect("read");
    let data = &sensitive.data;
    assert_eq!(
        data["warnings"],
        serde_json::json!(["1 line matches ignoring case."]),
        "{data}"
    );
    assert!(data.get("hints").is_none(), "{data}");
    let row =
        &crate::tools::test_support::checks::related_call(data, "ghGetFileContent", |input| {
            input["queries"][0]["caseMode"] == "insensitive"
        })["input"]["queries"][0];
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            data,
            "ghGetFileContent",
            |input| input["queries"][0]["caseMode"] == "insensitive"
        )["tool"],
        "ghGetFileContent"
    );
    assert_eq!(row["caseMode"], "insensitive", "{data}");
    assert_eq!(row["matchString"], json!(["Two"]), "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    crate::tools::test_support::validate_sealed(
        "ghGetFileContent",
        &serde_json::json!({"results":[{"index":0,"status":"empty","data":data}]}),
    )
    .expect("an ignoreCase lead satisfies the output contract");
}

/// A miss offers only reads that differ from the request and that select
/// lines of the redacted text: no ignoreCase lead for a read already
/// case-insensitive, none for a hit hidden behind a redaction, and a
/// `ranges` miss with hits elsewhere in this file leads to the same read
/// without `ranges`, not back to a repository search.
#[tokio::test]
async fn a_miss_leads_only_to_reads_that_select_visible_lines() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\nfour needle\n")}),).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha});
    let with = |fields: Value| {
        let mut row = base.clone();
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        row
    };
    for fields in [
        serde_json::json!({"matchString":["token"]}),
        serde_json::json!({"matchString":["TOKEN"],"caseMode":"insensitive"}),
    ] {
        let read = run_default(&provider, with(fields.clone()))
            .await
            .expect("read");
        let data = &read.data;
        assert_eq!(data["matchNotFound"], true, "{fields}: {data}");
        assert!(data["next"].get("ignoreCase").is_none(), "{fields}: {data}");
        assert!(data.get("warnings").is_none(), "{fields}: {data}");
    }
    let ranged = run_default(
        &provider,
        with(
            serde_json::json!({"matchString":["needle"],"ranges":[{"startLine": 1, "endLine": 2}]}),
        ),
    )
    .await
    .expect("read");
    let data = &ranged.data;
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(
        data["warnings"],
        serde_json::json!(["1 line matches outside the requested ranges."]),
        "{data}"
    );
    let lead = crate::tools::test_support::checks::related_call(data, "ghGetFileContent", |_| true);
    assert_eq!(lead["tool"], "ghGetFileContent", "{data}");
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["matchString"], json!(["needle"]), "{data}");
    assert!(row.get("ranges").is_none(), "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    assert!(data["next"].get("findCode").is_none(), "{data}");
}

/// A literal that a large file does not contain leads to a repository
/// code search for it (it may live in another file), never to a
/// semantic locate of the same file.
#[tokio::test]
async fn no_match_in_a_large_file_leads_to_a_repository_search() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = (0..1200).map(|n| format!("line {n}\n")).collect::<String>();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&text)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha,"matchString":["absent"]}),
    )
    .await
    .expect("read");
    let data = &read.data;
    assert!(data["next"].get("clasify").is_none(), "{data}");
    let lead = crate::tools::test_support::checks::related_call(data, "ghSearchCode", |_| true);
    assert_eq!(lead["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        lead["input"]["queries"][0]["keywords"],
        serde_json::json!(["absent"]),
        "{data}"
    );
}
