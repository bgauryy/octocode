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
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":"absent"}),
    )
    .await
    .expect("read");
    let data = &read.output.data;
    assert_eq!(read.output.status, Some("empty"), "{data}");
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(data["content"], "", "{data}");
    crate::contracts::validate_output(
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
    // FIX §0 #4: the runnable lead is the next step: a repository search
    // for a small file.
    let lead = &data["next"]["findCode"];
    assert_eq!(lead["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        lead["query"]["queries"][0]["keywords"],
        serde_json::json!(["absent"]),
        "{data}"
    );
    // The search stays at the read's ref, never the default branch.
    assert_eq!(lead["query"]["queries"][0]["ref"], sha, "{data}");
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
                lead["query"]["queries"][0]["path"], "a.txt",
                "{name} reads another path: {data}"
            );
        }
    }

    // A case-sensitive miss that a case-insensitive read selects runs
    // the same read, ignoring case, at the read's commit; the count of
    // lines it selects is the fact beside the lead.
    let sensitive = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":"Two"}),
    )
    .await
    .expect("read");
    let data = &sensitive.output.data;
    assert_eq!(
        data["warnings"],
        serde_json::json!(["1 line matches ignoring case."]),
        "{data}"
    );
    assert!(data.get("hints").is_none(), "{data}");
    let row = &data["next"]["ignoreCase"]["query"]["queries"][0];
    assert_eq!(data["next"]["ignoreCase"]["tool"], "ghGetFileContent");
    assert_eq!(row["caseMode"], "insensitive", "{data}");
    assert_eq!(row["matchString"], "Two", "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    crate::contracts::validate_output(
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
        serde_json::json!({"matchString":"token"}),
        serde_json::json!({"matchString":"TOKEN","caseMode":"insensitive"}),
    ] {
        let read = run_default(&provider, with(fields.clone()))
            .await
            .expect("read");
        let data = &read.output.data;
        assert_eq!(data["matchNotFound"], true, "{fields}: {data}");
        assert!(data["next"].get("ignoreCase").is_none(), "{fields}: {data}");
        assert!(data.get("warnings").is_none(), "{fields}: {data}");
    }
    let ranged = run_default(
        &provider,
        with(serde_json::json!({"matchString":"needle","ranges":["1-2"]})),
    )
    .await
    .expect("read");
    let data = &ranged.output.data;
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(
        data["warnings"],
        serde_json::json!(["1 line matches outside the requested ranges."]),
        "{data}"
    );
    let lead = &data["next"]["read"];
    assert_eq!(lead["tool"], "ghGetFileContent", "{data}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["matchString"], "needle", "{data}");
    assert!(row.get("ranges").is_none(), "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    assert!(data["next"].get("findCode").is_none(), "{data}");
}

/// The fallback tip follows the query: only a match read is told about
/// matchString.
#[test]
fn the_fallback_hint_names_matchstring_only_for_a_match_read() {
    use crate::response::output::ToolOutput;
    let plain = Output.fallback_hint(&serde_json::json!({"path":"a.js","ranges":["1-3"]}));
    assert!(!plain.contains("matchString"), "{plain}");
    let matched = Output.fallback_hint(&serde_json::json!({"path":"a.js","matchString":"x"}));
    assert!(matched.contains("matchString"), "{matched}");
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
        serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha,"matchString":"absent"}),
    )
    .await
    .expect("read");
    let data = &read.output.data;
    assert!(data["next"].get("clasify").is_none(), "{data}");
    let lead = &data["next"]["findCode"];
    assert_eq!(lead["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        lead["query"]["queries"][0]["keywords"],
        serde_json::json!(["absent"]),
        "{data}"
    );
}
