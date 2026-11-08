//! Bodies are redacted whole before they are matched or windowed: a token
//! never straddles a window edge, a `matchString` never lands inside one, and
//! walking every window rejoins the redacted body exactly once.
use super::*;
use crate::security::ContentSecurity;

fn secret() -> String {
    format!("ghp_{}", "Zq7".repeat(12))
}

/// Part of the secret no placeholder keeps.
fn leaks(text: &str) -> bool {
    let secret = secret();
    (0..=secret.len() - 12).any(|at| text.contains(&secret[at..at + 12]))
}

async fn read_secure(server: &MockServer, row: Value) -> Value {
    read_secure_in(server, row, None).await
}

/// `read_secure` with the response page `page_chars` wide (`None`: default).
async fn read_secure_in(server: &MockServer, row: Value, page_chars: Option<usize>) -> Value {
    let provider = mock_provider(server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let mut request = HistoryItemRequest::from_row(row).expect("valid row");
    request.auto_page_chars = page_chars;
    execute(&provider, &request, &context, &ContentSecurity::new())
        .await
        .expect("history item")
        .data
}

async fn mount_issue(server: &MockServer, body: &str) {
    mount_json(
        server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Crash","state":"open","user":{"login":"bob"},
            "body":body,"comments":0,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
}

/// Every window of the issue body, read through its `nextOffset`.
async fn walk_issue_body(server: &MockServer) -> Vec<String> {
    let mut windows = Vec::new();
    let mut offset = None::<u64>;
    loop {
        let mut row = json!({"operation":"issue","owner":"o","repo":"r","number":7,
            "sections":["body"]});
        if let Some(offset) = offset {
            row["offset"] = json!(offset);
        }
        let out = read_secure(server, row).await;
        windows.push(
            out["issues"][0]["body"]
                .as_str()
                .expect("body window")
                .to_owned(),
        );
        let page = out["issues"][0]["contentPagination"]["body"].clone();
        // The executable continuation, copied as the tool emits it.
        let next = &page["next"]["nextPage"]["input"]["queries"][0];
        match next["offset"].as_u64() {
            Some(next) if page["hasMore"] == true => {
                assert!(offset.is_none_or(|before| next > before), "advances: {out}");
                offset = Some(next);
            }
            _ => return windows,
        }
        assert!(windows.len() < 20, "walk ends");
    }
}

/// A token that crosses the window edge shows in part on neither page, and
/// the pages rejoin the redacted body exactly once.
#[tokio::test]
async fn a_token_across_a_window_edge_is_redacted_whole() {
    let window = super::super::DEFAULT_TEXT_WINDOW;
    let server = MockServer::builder().start().await;
    // The edge falls at several depths inside the token.
    for depth in [4, 10, 20, 30] {
        let body = format!(
            "{}{}{}",
            "a ".repeat((window - depth) / 2),
            secret(),
            " tail".repeat(10)
        );
        server.reset().await;
        mount_issue(&server, &body).await;
        let windows = walk_issue_body(&server).await;
        assert!(windows.len() > 1, "depth {depth}: cut");
        for text in &windows {
            assert!(!leaks(text), "depth {depth}: {text}");
        }
        let joined = windows.concat();
        assert!(joined.contains("[REDACTED"), "depth {depth}: {joined}");
        assert_eq!(joined.matches("tail").count(), 10, "depth {depth}");
        assert_eq!(joined.matches("[REDACTED").count(), 1, "depth {depth}");
        assert!(joined.starts_with("a a a "), "depth {depth}");
    }
}

/// Offsets count the redacted view: a window never ends inside a marker, and
/// the second page starts where the first ended.
#[tokio::test]
async fn offsets_count_the_redacted_body_and_windows_keep_markers_whole() {
    let window = super::super::DEFAULT_TEXT_WINDOW;
    let server = MockServer::builder().start().await;
    let body = format!("{} {} end", "z".repeat(window - 6), secret());
    mount_issue(&server, &body).await;
    let windows = walk_issue_body(&server).await;
    assert_eq!(windows.len(), 2, "{windows:?}");
    // The edge fell inside the marker: the first window ends before it, the
    // second starts with it, and the offsets count the redacted view.
    assert_eq!(windows[0], format!("{} ", "z".repeat(window - 6)));
    assert!(windows[1].starts_with("[REDACTED"), "{windows:?}");
    assert!(windows[1].ends_with(" end"), "{windows:?}");
}

/// A `matchString` inside a secret finds nothing: it runs on the redacted
/// text, so the secret's bytes are neither matched nor echoed.
#[tokio::test]
async fn a_match_string_inside_a_secret_matches_nothing() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
            "changed_files":0,"additions":0,"deletions":0,
            "created_at":"2024-01-01T00:00:00Z","comments":2}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/1/comments",
        200,
        json!([
            {"id":1,"user":{"login":"a"},"body":format!("token {}", secret()),
                "created_at":"2024-01-01T00:00:00Z"},
            {"id":2,"user":{"login":"b"},"body":"plain note",
                "created_at":"2024-01-01T00:00:00Z"}
        ]),
    )
    .await;
    let inside = &secret()[8..20];
    let out = read_secure(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments"],"matchString":[inside]}),
    )
    .await;
    let comments = out["pullRequests"][0]["comments"].as_array();
    assert!(comments.is_none_or(Vec::is_empty), "{out}");
    assert!(!leaks(&out.to_string()), "{out}");
    // The same read matching the redacted marker's neighbour still finds the comment.
    let out = read_secure(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments"],"matchString":["token"]}),
    )
    .await;
    let shown = out["pullRequests"][0]["comments"].to_string();
    assert!(shown.contains("[REDACTED"), "{out}");
    assert!(!leaks(&shown), "{out}");
}

/// The redaction the read made states itself on the call.
#[tokio::test]
async fn a_redacted_body_raises_the_redaction_warning() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, &format!("key {}", secret())).await;
    let out = read_secure(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":7}),
    )
    .await;
    assert!(
        out["warnings"].to_string().contains("redactedContent"),
        "{out}"
    );
}

/// Comment bodies of one issue page share one `offset`. A marker that
/// shortens one body's window must not make a sibling repeat or lose text:
/// walking every page rejoins each redacted body exactly once.
#[tokio::test]
async fn bodies_of_one_page_rejoin_exactly_once_when_a_marker_cuts_one() {
    let window = super::super::DEFAULT_TEXT_WINDOW;
    let straddling = format!(
        "{}{} {}",
        "a ".repeat((window - 10) / 2),
        secret(),
        "tail ".repeat(10)
    );
    let plain = "b ".repeat(window * 3 / 4);
    let short = "short note".to_owned();
    for order in [[0, 1, 2], [1, 0, 2], [2, 1, 0]] {
        let bodies = [&straddling, &plain, &short];
        let comments = order
            .iter()
            .map(|&at| {
                json!({"id": at + 1, "user": {"login": "u"}, "body": bodies[at],
                    "created_at": "2024-01-01T00:00:00Z"})
            })
            .collect::<Vec<_>>();
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/issues/7",
            200,
            json!({"number":7,"title":"Crash","state":"open","user":{"login":"bob"},
                "body":"b","comments":3,"created_at":"2024-01-01T00:00:00Z",
                "updated_at":"2024-01-02T00:00:00Z"}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/issues/7/comments",
            200,
            json!(comments),
        )
        .await;
        let mut row = json!({"operation":"issue","owner":"o","repo":"r","number":7,
            "sections":["comments"]});
        let mut joined = std::collections::BTreeMap::<u64, String>::new();
        for _ in 0..12 {
            let out = read_secure_in(&server, row.clone(), Some(1 << 20)).await;
            let issue = &out["issues"][0];
            for comment in issue["comments"].as_array().into_iter().flatten() {
                let id = comment["id"]
                    .as_str()
                    .expect("id")
                    .parse::<u64>()
                    .expect("number");
                joined
                    .entry(id)
                    .or_default()
                    .push_str(comment["body"].as_str().unwrap_or(""));
            }
            // Body windows first, then the next comment page, as emitted.
            let page = &issue["contentPagination"];
            let continuation =
                |axis: &str| page[axis]["next"]["nextPage"]["input"]["queries"][0].clone();
            let mut next = continuation("commentBody");
            if next.is_null() {
                next = continuation("comments");
            }
            if next.is_null() {
                break;
            }
            row = next;
        }
        let redacted = |text: &str| text.replace(&secret(), "[REDACTED-GITHUBTOKENS]");
        for (at, body) in bodies.iter().enumerate() {
            let id = u64::try_from(at).expect("id") + 1;
            assert_eq!(
                joined.get(&id),
                Some(&redacted(body)),
                "order {order:?} comment {id}"
            );
        }
    }
}

/// A pull request's discussion comment and review share the page's body
/// `offset`: a marker that cuts the review's window moves neither body's
/// text, and walking every page rejoins both exactly once.
#[tokio::test]
async fn pull_request_comment_and_review_bodies_rejoin_exactly_once() {
    let window = super::super::DEFAULT_TEXT_WINDOW;
    let marked = format!(
        "{}{} {}",
        "a ".repeat((window - 10) / 2),
        secret(),
        "tail ".repeat(10)
    );
    let plain = "b ".repeat(window * 3 / 4);
    for (comment, review) in [(&plain, &marked), (&marked, &plain)] {
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/pulls/1",
            200,
            json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
                "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
                "changed_files":0,"additions":0,"deletions":0,
                "created_at":"2024-01-01T00:00:00Z","comments":1}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/issues/1/comments",
            200,
            json!([{"id":1,"user":{"login":"a"},"body":comment,
                "created_at":"2024-01-01T00:00:00Z"}]),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/pulls/1/reviews",
            200,
            json!([{"id":2,"user":{"login":"b"},"state":"COMMENTED","body":review,
                "submitted_at":"2024-01-01T00:00:00Z","commit_id":HEAD}]),
        )
        .await;
        // Every continuation the pages emit, each walked once.
        let mut pending = vec![json!({"operation":"pullRequest","owner":"o","repo":"r",
            "number":1,"sections":["comments","reviews"]})];
        let (mut got_comment, mut got_review) = (String::new(), String::new());
        while let Some(row) = pending.pop() {
            assert!(pending.len() < 8, "walk ends");
            let out = read_secure_in(&server, row, Some(1 << 20)).await;
            let pr = &out["pullRequests"][0];
            for (list, into) in [("comments", &mut got_comment), ("reviews", &mut got_review)] {
                for item in pr[list].as_array().into_iter().flatten() {
                    into.push_str(item["body"].as_str().unwrap_or(""));
                }
            }
            for axis in ["commentBody", "reviewBody"] {
                let next =
                    &pr["contentPagination"][axis]["next"]["nextPage"]["input"]["queries"][0];
                if !next.is_null() {
                    pending.push(next.clone());
                }
            }
        }
        let redacted = |text: &str| text.replace(&secret(), "[REDACTED-GITHUBTOKENS]");
        assert_eq!(got_comment, redacted(comment), "comment");
        assert_eq!(got_review, redacted(review), "review");
    }
}

/// A `matchString` read whose `contextLines` reach past the hunk widens it
/// from the file's head text; that text is source text too, so a dotenv
/// credential line it brings in is redacted with the file's path before the
/// view numbers it, like the patch itself. By default the policy withholds the
/// file, so its head text is not read at all and the clipped runs are flagged.
#[tokio::test]
async fn head_text_widening_skips_a_withheld_file_by_default() {
    let (out, reads) = widen_dotenv(false, &ContentSecurity::new()).await;
    let shown = out.to_string();
    assert_eq!(reads, 0, "a withheld head file is not read: {out}");
    assert!(!shown.contains("SETTING_2=on"), "no head text: {out}");
    assert!(shown.contains("DEBUG=true"), "the patch stays: {out}");
    assert!(shown.contains("\"contextClipped\":true"), "{out}");
}

/// Refuses the head text of the widened file (the only text holding
/// `SETTING_1=`), as the scanner refuses an oversized text; scans the rest.
struct RefusingHead(ContentSecurity);

impl crate::security::scan::ContentScan for RefusingHead {
    fn sanitize(
        &self,
        text: &str,
        path: &std::path::Path,
    ) -> Result<(String, Vec<String>), (octocode_contract_types::ErrorCode, String)> {
        if text.contains("SETTING_1=") {
            return Err((
                octocode_contract_types::ErrorCode::ContentSecurityLimit,
                "refused".into(),
            ));
        }
        self.0.sanitize(text, path)
    }
}

#[tokio::test]
async fn head_text_the_scanner_refuses_counts_as_unread() {
    let (out, reads) = widen_dotenv(true, &RefusingHead(ContentSecurity::new())).await;
    let shown = out.to_string();
    assert_eq!(reads, 1, "{out}");
    assert!(!shown.contains("SETTING_2=on"), "{out}");
    assert!(shown.contains("\"contextClipped\":true"), "{out}");
}

/// A pull request whose `.env` patch hunk a `matchString` read widens from
/// the head text, read under a policy that opens credential names (`allow`)
/// or not, scanned by `security`: the row, and the head file reads it made.
async fn widen_dotenv(
    allow: bool,
    security: &(impl crate::security::scan::ContentScan + Sync),
) -> (Value, usize) {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"Debug","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
            "changed_files":1,"additions":1,"deletions":1,
            "created_at":"2024-01-01T00:00:00Z","comments":0}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/files",
        200,
        json!([{"filename":".env","status":"modified","additions":1,"deletions":1,
            "patch":"@@ -10,2 +10,2 @@\n APP_NAME=demo\n-DEBUG=false\n+DEBUG=true"}]),
    )
    .await;
    let head: String = (1..=20)
        .map(|line| match line {
            3 => "DB_PASS=CowSaysMoo\n".to_owned(),
            10 => "APP_NAME=demo\n".to_owned(),
            11 => "DEBUG=true\n".to_owned(),
            18 => "API_TOKEN=Moo12\n".to_owned(),
            _ => format!("SETTING_{line}=on\n"),
        })
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/o/r/contents/.env",
        200,
        json!({"type":"file","encoding":"base64",
            "content":base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &head)}),
    )
    .await;
    let mut request = HistoryItemRequest::from_row(json!({"operation":"pullRequest","owner":"o",
        "repo":"r","number":1,"sections":["patches"],"matchString":["DEBUG"],
        "contextLines":{"before":9,"after":9}}))
    .expect("valid row");
    let root = tempfile::tempdir().expect("root");
    let policy = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        allow_credential_files: allow,
        ..Default::default()
    })
    .expect("policy");
    request.paths = Some(std::sync::Arc::new(policy));
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let out = execute(&provider, &request, &context, security)
        .await
        .expect("history item")
        .data;
    let reads = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| {
            let path = request.url.path();
            path.contains("/contents/") || path.contains("/git/blobs/")
        })
        .count();
    (out, reads)
}

#[tokio::test]
async fn head_text_widening_a_dotenv_patch_hides_credential_values() {
    let (out, reads) = widen_dotenv(true, &ContentSecurity::new()).await;
    assert_eq!(reads, 1, "the opt-in reads the head file: {out}");
    let shown = out.to_string();
    assert!(
        shown.contains("SETTING_2=on"),
        "head text widened the hunk: {out}"
    );
    assert!(shown.contains("DEBUG=true"), "{out}");
    for secret in ["CowSaysMoo", "Moo12"] {
        assert!(!shown.contains(secret), "{secret} leaked: {out}");
    }
    for kept in ["DB_PASS=", "API_TOKEN="] {
        assert!(shown.contains(kept), "{kept} lost: {out}");
    }
}

/// A summary's `bodyPreview` is cut from the redacted body: a token near
/// the preview's edge never shows raw, nor does the preview end inside a
/// redaction marker, whichever character the cut falls on.
///
/// The raw-token check guards the order (fetch-time redaction, then the cut):
/// it fails only if a body reaches `body_preview` unredacted. The marker
/// check is the one that failed before `clean_char_cut` (at `before` 277).
#[tokio::test]
async fn a_body_preview_shows_no_raw_token_and_no_partial_marker() {
    for before in 270..300 {
        let body = format!("{} {} tail words", "a".repeat(before), secret());
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/pulls/1",
            200,
            json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
                "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
                "changed_files":0,"additions":0,"deletions":0,"body":body,
                "created_at":"2024-01-01T00:00:00Z"}),
        )
        .await;
        let out = read_secure(
            &server,
            json!({"operation":"pullRequest","owner":"o","repo":"r","number":1}),
        )
        .await;
        let preview = out["pullRequests"][0]["bodyPreview"]
            .as_str()
            .unwrap_or_else(|| panic!("no preview: {out}"));
        assert!(!leaks(preview), "before {before}: {preview}");
        let text = preview.trim_end_matches('…');
        assert!(
            text.rfind("[REDACTED")
                .map(|open| text[open..].contains(']'))
                != Some(false),
            "before {before}: the preview ends inside a marker: {preview}"
        );
    }
}
