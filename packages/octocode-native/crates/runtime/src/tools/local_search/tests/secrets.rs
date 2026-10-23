use super::*;
use octocode_contract_types::ErrorCode;

#[test]
fn clipped_secret_guard_fails_closed_when_the_source_cannot_be_reread() {
    let security = ContentSecurity::new();
    let mut file = octocode_engine::types::TextSearchFile {
        path: "gone.txt".into(),
        match_count: 1,
        matches: vec![octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 1,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: "…token = ghp_abcdefghijklmnop".into(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(
                "…token = ghp_abcdefghijklmnop".as_bytes(),
            )
            .into(),
            count: None,
            rank: None,
            original_chars: Some(400),

            original_bytes: Some(400),
        }],
        source: None,
    };
    let missing = tempfile::tempdir()
        .expect("fixture directory")
        .path()
        .join("gone.txt");
    let verified = verify::guard_clipped_secrets(
        &mut file,
        &missing,
        None,
        0..10,
        &verify::ValueCheck {
            security: &security,
            match_only: false,
            cancel: &NeverCancel,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(!file.matches[0].value.contains("ghp_"));
    assert!(file.matches[0].value.contains("REDACTED"));
}

/// A source that shrank or was replaced after the search no longer holds
/// the matched line, so the clipped value cannot be verified.
#[test]
fn clipped_secret_guard_fails_closed_when_the_match_line_is_gone() {
    let security = ContentSecurity::new();
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("shrunk.txt");
    fs::write(&source, "one line now\n").expect("fixture");
    let mut file = octocode_engine::types::TextSearchFile {
        path: "shrunk.txt".into(),
        match_count: 1,
        matches: vec![octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 40,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: "…token = ghp_abcdefghijklmnop".into(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(
                "…token = ghp_abcdefghijklmnop".as_bytes(),
            )
            .into(),
            count: None,
            rank: None,
            original_chars: Some(400),

            original_bytes: Some(400),
        }],
        source: None,
    };
    let verified = verify::guard_clipped_secrets(
        &mut file,
        &source,
        None,
        0..10,
        &verify::ValueCheck {
            security: &security,
            match_only: false,
            cancel: &NeverCancel,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(
        !file.matches[0].value.contains("ghp_"),
        "{}",
        file.matches[0].value
    );
}

/// Make `dir` unreadable; returns false when the process can still read it
/// (e.g. running as root), so the caller skips.
#[cfg(unix)]
fn lock_dir(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");
    if fs::read_dir(dir).is_ok() {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).expect("restore");
        return false;
    }
    true
}

/// Every page a localSearch chain reaches when every offered `next` page
/// route is followed, breadth-first from `request`.
struct PageWalk {
    pages: usize,
    /// Each page's page-route names, in walk order.
    routes: Vec<Vec<String>>,
    /// Every shown row, `(path, line)`.
    rows: std::collections::HashSet<(String, u32)>,
}

#[cfg(unix)]
fn unlock_dir(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
}

/// `root/hidden/secret.txt` holding the only `needle-only-here`, with
/// `hidden` made unreadable; `None` when the process can still read it.
#[cfg(unix)]
fn locked_secret(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let hidden = root.join("includeHidden");
    fs::create_dir(&hidden).expect("dir");
    fs::write(hidden.join("secret.txt"), "needle-only-here\n").expect("fixture");
    lock_dir(&hidden).then_some(hidden)
}

fn numbered(lines: u32, needles: &[u32]) -> String {
    (1..=lines)
        .map(|n| {
            if needles.contains(&n) {
                format!("line {n} needle\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

fn ranked_paths(files: &[(&str, &str)], fields: serde_json::Value) -> Vec<String> {
    let body = search_fixture(files, ls_query(fields, None));
    body["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// Follow every `nextPage`/`nextMatchPage` of every page (breadth-first),
/// asserting that only the `nextPage` a group-ending match page re-offers
/// repeats an executable query, and no row is shown twice.
fn walk_every_page_route(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> PageWalk {
    let mut queue = std::collections::VecDeque::from([request.clone()]);
    let mut offered = std::collections::HashSet::new();
    let mut walk = PageWalk {
        pages: 0,
        routes: Vec::new(),
        rows: std::collections::HashSet::new(),
    };
    while let Some(request) = queue.pop_front() {
        let result =
            select_all_search(&request, policy, security, &NeverCancel, None, None).expect("page");
        assert!(
            !result.terminal_limit,
            "a complete scan's exhausted branch is not a terminal limit: {result:?}"
        );
        if local_search_query::page(&request)
            < result
                .pagination
                .as_ref()
                .map_or(1, |page| page.total_pages)
        {
            assert_eq!(result.status, SearchStatus::Partial, "{result:?}");
        }
        walk.pages += 1;
        assert!(walk.pages < 50, "pagination must terminate");
        let match_page = local_search_query::match_page(&request).max(1);
        for file in &result.files {
            for row in file.matches.iter().flatten() {
                assert!(
                    walk.rows.insert((file.path.clone(), row.line)),
                    "repeated evidence {}:{} at page {} matchPage {match_page}",
                    file.path,
                    row.line,
                    local_search_query::page(&request)
                );
            }
        }
        let next = result.next.unwrap_or_default();
        let mut names = Vec::new();
        for name in ["nextMatchPage", "nextPage"] {
            let Some(route) = next.get(name) else {
                continue;
            };
            names.push(name.to_owned());
            let query = route["query"]["queries"][0].clone();
            if !offered.insert(crate::tools::test_support::checks::canonical(&query)) {
                assert!(
                    name == "nextPage" && match_page > 1 && next.get("nextMatchPage").is_none(),
                    "duplicate page route {name} at page {} matchPage {match_page}: {query}",
                    local_search_query::page(&request)
                );
                continue;
            }
            queue.push_back(serde_json::from_value(query).expect("continuation query"));
        }
        walk.routes.push(names);
    }
    walk
}

fn all_values(body: &serde_json::Value) -> String {
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|f| f["matches"].as_array().cloned().unwrap_or_default())
        .map(|m| m["value"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

// A clipped value (matchOnly span, matchContentLength) must not
// leak a partial secret that no longer matches the secret patterns.
#[test]
fn clipped_values_never_leak_partial_secrets() {
    let token = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
    let file = format!("token: {token} end\n");
    let cases = [
        ls_query(
            serde_json::json!({"matchString": "ghp_[0-9a-z]{20}".to_string(), "regex": "rust", "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": "token".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly, "matchContentLength": 4}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": "end".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly, "matchContentLength": 2}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": "token".to_string(), "matchContentLength": 30}),
            None,
        ),
    ];
    for request in cases {
        let body = search_fixture(&[("sec.txt", &file)], request);
        let values = all_values(&body);
        assert!(!values.is_empty());
        for fragment in ["1234567890", "klmnopqrstuvwxyz"] {
            assert!(!values.contains(fragment), "leaked {fragment}: {values}");
        }
    }
    // Clean lines in the same file keep their exact values.
    let body = search_fixture(
        &[("sec.txt", &format!("{file}plain needle line\n"))],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    assert_eq!(all_values(&body), "needle");
}

#[test]
fn short_spans_inside_private_key_bodies_are_redacted() {
    let file = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n";
    let body = search_fixture(
        &[("key.rs", file)],
        ls_query(
            serde_json::json!({"matchString": "interior".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    assert!(!all_values(&body).contains("interior"), "{body}");
}

/// A literal marker string in ordinary source opens a block that never
/// closes; the code after it must stay readable, while a real key's body
/// (here also unterminated) stays redacted.
#[test]
fn code_after_a_literal_key_marker_stays_readable() {
    let literal = "const PEM = \"-----BEGIN RSA PRIVATE KEY-----\";\nlet needle_one = compute();\nfn needle_two() {}\n";
    let body = search_fixture(
        &[("src.rs", literal)],
        ls_query(
            serde_json::json!({"matchString": "needle_", "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    assert_eq!(all_values(&body), "needle_\nneedle_", "{body}");
    assert!(!all_values(&body).contains("REDACTED"), "{body}");
    let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nfn needle_after() {}\n";
    let body = search_fixture(
        &[("key.rs", key)],
        ls_query(
            serde_json::json!({"matchString": "interior|needle_after", "regex": "rust", "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let values = all_values(&body);
    assert!(!values.contains("interior"), "{body}");
    assert!(values.contains("needle_after"), "{body}");
}

/// Sources above the whole-file key-scan cap are verified by the streamed
/// prefix pass: late interior key bodies stay redacted (terminated or
/// not), while innocent base64 outside any block stays readable.
#[test]
fn late_key_bodies_in_large_sources_stay_redacted() {
    let filler = "let filler_value = 0;\n".repeat(560_000);
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let innocent = "QUJDREVGR0hJSktMTU5PUFFSU1RVinnocentB64VldYWVowMTIzNDU2Nzg5";
    let terminated = format!(
        "{innocent}\n{filler}-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\n"
    );
    let unterminated =
        format!("{innocent}\n{filler}-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n");
    for content in [&terminated, &unterminated] {
        assert!(content.len() as u64 > 10 * 1024 * 1024);
        for view in ["matchOnly", "paginated"] {
            let found = search_fixture(
                &[("big.txt", content.as_str())],
                ls_query(
                    serde_json::json!({"matchString": "interior", "resultView": view, "contextLines": 0}),
                    None,
                ),
            );
            assert_eq!(found["files"][0]["matches"][0]["line"], 560_003, "{found}");
            assert!(!all_values(&found).contains("interior"), "{found}");
        }
        let readable = search_fixture(
            &[("big.txt", content.as_str())],
            ls_query(
                serde_json::json!({"matchString": "innocentB64", "resultView": "paginated", "contextLines": 0}),
                None,
            ),
        );
        assert!(all_values(&readable).contains(innocent), "{readable}");
    }
}

#[test]
fn next_page_restarts_match_rows_and_next_match_page_tracks_shown_files() {
    let many = "foo\n".repeat(5);
    let files = [
        ("a.txt", many.as_str()),
        ("b.txt", "foo\n"),
        ("c.txt", many.as_str()),
    ];
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 2, "matchPage": 3, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    // Page 1 at matchPage 3: a.txt's 5th row is shown, nothing is left over.
    assert!(
        body["next"].get("nextMatchPage").is_none(),
        "{}",
        body["next"]
    );
    assert_eq!(
        body["next"]["nextPage"]["query"]["queries"][0]["matchPage"],
        1
    );
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 2, "page": 2, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    assert_eq!(
        body["next"]["nextMatchPage"]["query"]["queries"][0]["matchPage"],
        2
    );
}

#[test]
fn two_axis_pages_form_one_exact_once_continuation_chain() {
    let root = tempfile::tempdir().expect("fixture");
    for (name, count) in [("a.txt", 3), ("b.txt", 2), ("c.txt", 2)] {
        std::fs::write(root.path().join(name), "foo\n".repeat(count)).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path":root.path(), "matchString":"foo", "pageSize":2,
            "matchPageSize":2, "sort":"path", "contextLines":0,
            "clasifyContext":{"classifiers":["find every foo row"]}
        }),
        None,
    );
    let walk = walk_every_page_route(&request, &policy, &security);
    // The first page has rows left in its files and files left after them:
    // both routes are offered, depth-first and breadth-first.
    assert_eq!(
        walk.routes[0],
        ["nextMatchPage", "nextPage"],
        "{:?}",
        walk.routes
    );
    assert_eq!(walk.pages, 3);
    assert_eq!(walk.rows.len(), 7);
}

/// Breadth-first and depth-first access: `nextPage` rides beside
/// `nextMatchPage` on the page that opens a file group (matchPage 1) and
/// returns on the page that ends it, so no page with later files is a dead
/// end; following every distinct offered route shows each row once.
#[test]
fn every_offered_page_route_is_one_executable_query_and_rows_show_once() {
    let root = tempfile::tempdir().expect("fixture");
    let files = [
        ("a.txt", 5),
        ("b.txt", 3),
        ("c.txt", 1),
        ("d.txt", 4),
        ("e.txt", 2),
    ];
    for (name, count) in files {
        std::fs::write(root.path().join(name), "foo\n".repeat(count)).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path":root.path(), "matchString":"foo", "pageSize":2,
            "matchPageSize":2, "sort":"path", "contextLines":0
        }),
        None,
    );
    let walk = walk_every_page_route(&request, &policy, &security);
    let total: usize = files.iter().map(|(_, count)| count).sum();
    assert_eq!(walk.rows.len(), total, "every row once");
    // File pages {a,b} {c,d} {e}: three match pages, then two, then one.
    assert_eq!(walk.pages, 6, "{:?}", walk.routes);
    // A middle match page never re-offers the next file page; the match
    // page that ends a file group does (a repeat the walk does not rerun).
    assert_eq!(
        walk.routes,
        [
            vec!["nextMatchPage", "nextPage"],
            vec!["nextMatchPage"],
            vec!["nextMatchPage", "nextPage"],
            vec!["nextPage"],
            vec!["nextPage"],
            vec![],
        ]
    );
}

/// A grid page names `nextMatchPage` while rows remain in its files, then
/// `nextPage` for later files. Row totals live once, as `matchedLineCount`
/// beside the occurrence
/// count, and a later match page says which match page it shows.
#[test]
fn grid_pages_count_the_rows_a_match_page_reaches_and_name_the_match_page() {
    let many = "foo\n".repeat(5);
    let files = [
        ("a.txt", many.as_str()),
        ("b.txt", "foo foo\n"),
        ("c.txt", many.as_str()),
    ];
    let request = |match_page: u32| {
        ls_query(
            serde_json::json!({"matchString": "foo", "pageSize": 2, "matchPage": match_page, "matchPageSize": 2, "sort": LocalSearchQuerySort::Path}),
            None,
        )
    };
    let body = search_fixture(&files, request(1));
    // The shared page stage says how many rows `nextMatchPage` reaches.
    assert!(body.get("warnings").is_none(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_some(), "{body}");
    assert!(body["pagination"].get("matchCount").is_none(), "{body}");
    assert!(body["pagination"].get("matchPage").is_none(), "{body}");
    assert_eq!(body["matchedLineCount"], 11, "{body}");
    assert_eq!(body["matchCount"], 12, "{body}");
    let second = search_fixture(&files, request(2));
    assert_eq!(second["pagination"]["currentPage"], 1, "{second}");
    assert_eq!(second["pagination"]["matchPage"], 2, "{second}");
    assert!(second["next"].get("nextMatchPage").is_some(), "{second}");
}

#[test]
fn a_clipped_file_shows_its_deciding_hits_first_and_lists_the_rest() {
    // Comment and call hits come first by line; the assignment and the
    // branch that decide behaviour sit past the per-file cap.
    let mut file = String::new();
    for _ in 0..4 {
        file.push_str(" * maximumSize doc\n");
    }
    for _ in 0..3 {
        file.push_str("check(maximumSize);\n");
    }
    file.push_str("this.maximumSize = maximumSize;\n"); // line 8
    file.push_str("if (maximumSize > 0) {\n"); // line 9
    let request = |match_page| {
        ls_query(
            serde_json::json!({"matchString": "maximumSize", "matchPageSize": 3, "matchPage": match_page, "contextLines": 0}),
            None,
        )
    };
    let lines = |body: &serde_json::Value| -> Vec<u64> {
        body["files"][0]["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .filter_map(|m| m["line"].as_u64())
            .collect()
    };
    let first = search_fixture(&[("CacheBuilder.java", &file)], request(1));
    // Page 1 holds the best-ranked rows, shown in source order.
    assert_eq!(lines(&first), [5, 8, 9], "{first}");
    // The rows still unseen are named, cheap to read at their lines.
    assert_eq!(
        first["files"][0]["pagination"]["moreLines"],
        serde_json::json!([1, 2, 3, 4, 6, 7]),
        "{first}"
    );
    let second = search_fixture(&[("CacheBuilder.java", &file)], request(2));
    assert_eq!(lines(&second), [1, 6, 7], "{second}");
    assert_eq!(
        second["files"][0]["pagination"]["moreLines"],
        serde_json::json!([2, 3, 4])
    );
    let last = search_fixture(&[("CacheBuilder.java", &file)], request(3));
    assert_eq!(lines(&last), [2, 3, 4], "{last}");
    assert!(last["files"][0].get("pagination").is_none(), "{last}");
}

#[test]
fn a_repeated_row_follows_every_distinct_row_of_a_clipped_file() {
    // Identical impls of one trait method rank as declarations, but the
    // second copy adds nothing over the distinct call site.
    let file = "fn needle() {\nfn needle() {\ncall(needle);\n";
    let body = search_fixture(
        &[("a.rs", file)],
        ls_query(
            serde_json::json!({"matchString": "needle", "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    let lines: Vec<u64> = body["files"][0]["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .filter_map(|m| m["line"].as_u64())
        .collect();
    assert_eq!(lines, [1, 3], "{body}");
    assert_eq!(
        body["files"][0]["pagination"]["moreLines"],
        serde_json::json!([2])
    );
}

#[test]
fn line_rows_omit_the_column_that_span_rows_need() {
    let files = [("a.txt", "x needle needle\n")];
    let lines = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert!(
        lines["files"][0]["matches"][0].get("column").is_none(),
        "{lines}"
    );
    let spans = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "needle", "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let columns: Vec<u64> = spans["files"][0]["matches"]
        .as_array()
        .expect("spans")
        .iter()
        .filter_map(|m| m["column"].as_u64())
        .collect();
    assert_eq!(columns, [3, 10], "{spans}");
}

#[test]
fn later_match_pages_omit_files_exhausted_on_earlier_pages() {
    let many = "foo\n".repeat(5);
    let files = [("a.txt", many.as_str()), ("b.txt", "foo\n")];
    let request = |match_page| {
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPage": match_page, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        )
    };
    let body = search_fixture(&files, request(2));
    let paths: Vec<&str> = body["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert_eq!(paths, ["a.txt"], "{body}");
    // Every file exhausted: keep the out-of-range diagnostic rows.
    let body = search_fixture(&files, request(9));
    assert_eq!(body["files"][0]["pagination"]["outOfRange"], true, "{body}");
}

#[test]
fn list_views_have_no_match_rows_to_page() {
    let body = search_fixture(
        &[("a.txt", &"foo\n".repeat(15))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "resultView": LocalSearchQueryResultView::Files}),
            None,
        ),
    );
    assert!(body.get("next").is_none(), "{}", body["next"]);
    assert_ne!(body["status"], "partial", "{body}");
}

#[test]
fn binary_prefix_matches_mark_a_search_partial_and_terminal() {
    // Matches before the NUL do not prove the rest of the file was
    // searched: the cut is a coverage limit no continuation can lift.
    let body = search_fixture(
        &[("bin.dat", "foo\u{0}foo\n"), ("a.txt", "foo\n")],
        ls_query(serde_json::json!({"matchString": "foo".to_string()}), None),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert!(body["next"].get("nextPage").is_none(), "{body}");
    assert_eq!(
        body["partialReasons"],
        serde_json::json!(["binaryQuit"]),
        "{body}"
    );
    // localFetch rejects binary files, so a binary file is never the read:
    // the read takes the shown text hit.
    let read = &body["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{body}");
    assert_eq!(
        read["query"]["queries"][0]["path"]
            .as_str()
            .map(|path| path.ends_with("a.txt")),
        Some(true),
        "{body}"
    );
    assert!(!body.to_string().contains("bin.dat\",\"ranges"), "{body}");
}

#[test]
fn reverse_applies_to_default_relevance_order() {
    let files = [("a.txt", "foo\nfoo\nfoo\n"), ("b.txt", "foo\n")];
    let order = |reverse| {
        let body = search_fixture(
            &files,
            ls_query(
                serde_json::json!({"matchString": "foo".to_string(), "reverse": reverse}),
                None,
            ),
        );
        body["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(None), ["a.txt", "b.txt"]);
    assert_eq!(order(Some(true)), ["b.txt", "a.txt"]);
}

#[test]
fn relevance_ranks_a_declaration_hit_above_a_comment_or_string_hit_at_equal_count() {
    // Path order alone would put the comment and string hits first.
    let files = [
        ("a_comment.rs", "// parse_config reads the file\n"),
        ("b_string.rs", "let label = \"parse_config\";\n"),
        ("c_call.rs", "let cfg = parse_config(path);\n"),
        ("d_decl.rs", "pub fn parse_config(path: &str) -> Config {\n"),
    ];
    for view in ["paginated", "countMatches", "countLines", "matchOnly"] {
        assert_eq!(
            ranked_paths(
                &files,
                serde_json::json!({"matchString": "parse_config", "resultView": view}),
            ),
            ["d_decl.rs", "c_call.rs", "a_comment.rs", "b_string.rs"],
            "view {view}"
        );
    }
}

#[test]
fn relevance_ranks_a_source_file_above_test_and_generated_files_at_equal_count() {
    let hit = "let cfg = parse_config(path);\n";
    let files = [
        ("a/tests/config.rs", hit),
        ("b/config_test.go", hit),
        ("c/config.test.ts", hit),
        ("d/generated/config.rs", hit),
        ("e/vendor/config.go", hit),
        ("z/src/config.rs", hit),
    ];
    let ranked = ranked_paths(&files, serde_json::json!({"matchString": "parse_config"}));
    assert_eq!(ranked[0], "z/src/config.rs", "{ranked:?}");
    // The demoted rest keeps a total order: path breaks the tie, and a
    // generated file follows every hand-written one.
    assert_eq!(
        &ranked[1..],
        [
            "a/tests/config.rs",
            "b/config_test.go",
            "c/config.test.ts",
            "e/vendor/config.go",
            "d/generated/config.rs",
        ]
    );
    // Match count still dominates: two test hits beat one source hit.
    let ranked = ranked_paths(
        &[
            ("a/tests/config.rs", "parse_config();\nparse_config();\n"),
            ("z/src/config.rs", hit),
        ],
        serde_json::json!({"matchString": "parse_config"}),
    );
    assert_eq!(ranked, ["a/tests/config.rs", "z/src/config.rs"]);
}

#[test]
fn relevance_on_path_list_views_demotes_test_paths_and_path_sort_does_not() {
    let hit = "parse_config();\n";
    let files = [("a/tests/config.rs", hit), ("z/src/config.rs", hit)];
    let relevance = ranked_paths(
        &files,
        serde_json::json!({"matchString": "parse_config", "resultView": "files"}),
    );
    assert_eq!(relevance, ["z/src/config.rs", "a/tests/config.rs"]);
    let by_path = ranked_paths(
        &files,
        serde_json::json!({"matchString": "parse_config", "resultView": "files", "sort": "path"}),
    );
    assert_eq!(by_path, ["a/tests/config.rs", "z/src/config.rs"]);
    let without = ranked_paths(
        &[("a/tests/other.rs", "x\n"), ("z/src/other.rs", "x\n")],
        serde_json::json!({"matchString": "parse_config", "resultView": "filesWithout"}),
    );
    assert_eq!(without, ["z/src/other.rs", "a/tests/other.rs"]);
}

// Lean defaults: the paginated view returns only the hit line; a page takes
// each file's first 10 rows, then later rows while the window has room, so
// every file fits beside the hot one; `detailed` keeps a ±3 context window.
#[test]
fn default_paginated_view_is_lean_and_detailed_keeps_context() {
    let file = numbered(30, &(1..=12).collect::<Vec<_>>());
    let many: Vec<(String, String)> = (0..25)
        .map(|i| {
            (
                format!("f{i:02}.txt"),
                "line 1 needle\nline 2 needle\n".to_owned(),
            )
        })
        .collect();
    // A hot last file puts the result over one page budget.
    let hot = "needle\n".repeat(1200);
    let mut fixtures: Vec<(&str, &str)> = vec![("a.txt", file.as_str()), ("z.txt", &hot)];
    fixtures.extend(many.iter().map(|(p, c)| (p.as_str(), c.as_str())));
    let body = search_fixture(
        &fixtures,
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    let files = body["files"].as_array().expect("files");
    assert_eq!(files.len(), 27, "every file fits the window: {body}");
    let a = &files[0];
    assert_eq!(a["matches"].as_array().expect("rows").len(), 12, "{body}");
    assert_eq!(a["matches"][0]["value"], "line 1 needle");
    let hot = files
        .iter()
        .find(|file| file["path"] == "z.txt")
        .expect("hot file");
    // The file's total is its shown rows plus the later lines named and
    // counted: derivable, so not stated again.
    let pagination = &hot["pagination"];
    assert!(pagination.get("totalItems").is_none(), "{pagination}");
    let shown = hot["matches"].as_array().expect("rows").len();
    let listed = pagination["moreLines"].as_array().expect("moreLines").len();
    let unlisted = pagination["moreLinesUnlisted"].as_u64().unwrap_or(0) as usize;
    assert_eq!(shown + listed + unlisted, 1200, "{pagination}");
    assert!(shown < 1200);
    let detailed = search_fixture(
        &[("a.txt", &numbered(30, &[20]))],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 3}),
            None,
        ),
    );
    assert_eq!(
        detailed["files"][0]["matches"][0]["value"],
        "17\tline 17\n18\tline 18\n19\tline 19\n20\tline 20 needle\n21\tline 21\n22\tline 22\n23\tline 23"
    );
}

/// Following `nextPage` from a default search walks every row exactly
/// once in pages cut by the response budget: a hot file is not paged ten
/// rows at a time, and each continuation runs unchanged.
#[test]
fn a_default_walk_reaches_every_row_once_in_budget_sized_pages() {
    let root = tempfile::tempdir().expect("fixture directory");
    let rows_per_file = 60;
    for n in 0..30 {
        let body = (1..=rows_per_file)
            .map(|line| format!("needle {n} {line} {}\n", "x".repeat(60)))
            .collect::<String>();
        fs::write(root.path().join(format!("f{n:02}.txt")), body).expect("fixture");
    }
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    ))
    .expect("query");
    let (walk, bodies) = walk_search(root.path(), start, None);
    for (index, body) in bodies.iter().enumerate() {
        let chars = octocode_engine::json_size::utf16_units(&body["files"]).unwrap_or(0);
        assert!(
            chars <= crate::response::page_windows::page_budget(None, &serde_json::json!({})),
            "page {}: {chars} chars",
            index + 1
        );
        assert!(
            body["next"].get("nextMatchPage").is_none(),
            "{}",
            body["next"]
        );
        if let Some(next) = body["next"].get("nextPage") {
            assert!(
                next["query"]["queries"][0].get("pageSize").is_none(),
                "{next}"
            );
        }
    }
    // Each row once, every continuation run unchanged.
    assert_eq!(walk.rows.len(), 30 * rows_per_file);
    let pages = bodies.len();
    // About 30 × 60 rows of ~100 bytes: the overview page plus
    // total/budget pages, not one page per ten rows of each file.
    assert!((7..=11).contains(&pages), "{pages} pages");
}

// Overlapping/adjacent ±contextLines windows are emitted once: every source
// line appears exactly once, in order, and every matched line is recorded.
#[test]
fn overlapping_context_windows_merge_into_one_block() {
    let file = numbered(30, &[5, 7, 12, 25]);
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 2}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    // 5 and 7 overlap (3..=9), 12 is adjacent (10..=14 follows 9), 25 is apart.
    assert_eq!(matches.len(), 2, "{body}");
    assert_eq!(matches[0]["line"], 5);
    assert_eq!(matches[0]["matchedLines"], serde_json::json!([5, 7, 12]));
    let expected: String = (3..=14)
        .map(|n| {
            if [5, 7, 12].contains(&n) {
                format!("{n}\tline {n} needle")
            } else {
                format!("{n}\tline {n}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(matches[0]["value"], expected);
    assert_eq!(matches[1]["line"], 25);
    assert!(matches[1].get("matchedLines").is_none(), "{body}");
    assert_eq!(
        matches[1]["value"],
        "23\tline 23\n24\tline 24\n25\tline 25 needle\n26\tline 26\n27\tline 27"
    );
    // Counts stay per matched line.
    assert_eq!(body["matchedLineCount"], 4);
}

#[test]
fn windows_merge_at_file_edges_and_stay_apart_when_disjoint() {
    let file = numbered(6, &[1, 2, 6]);
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 1}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{body}");
    assert_eq!(
        matches[0]["value"],
        "1\tline 1 needle\n2\tline 2 needle\n3\tline 3"
    );
    assert_eq!(matches[0]["matchedLines"], serde_json::json!([1, 2]));
    assert_eq!(matches[1]["value"], "5\tline 5\n6\tline 6 needle");
    // matchOnly never merges: it carries spans, not windows.
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 3, "{body}");
    assert!(matches.iter().all(|m| m.get("matchedLines").is_none()));
}

// A truncated window must never be merged (its line structure is unknown).
#[test]
fn truncated_windows_are_not_merged() {
    let long = "x".repeat(200);
    let file = format!("{long}\nneedle a\n{long}\nneedle b\n{long}\n");
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "matchContentLength": 100}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{body}");
    assert!(matches.iter().all(|m| m.get("matchedLines").is_none()));
}

// Single-page results carry no redundant accounting: no engine constant,
// no per-file row counters, and a pagination block only where something
// continues (LS-3: rows left in a shown file count, so the block's `hasMore`
// agrees with `next.nextMatchPage`); per-file pagination only for a file
// with more match pages.
#[test]
fn single_page_results_omit_redundant_accounting() {
    let body = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", &"foo\n".repeat(3))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    assert!(body.get("searchEngine").is_none(), "{body}");
    assert_eq!(body["pagination"]["hasMore"], true, "{body}");
    assert_eq!(body["pagination"]["totalPages"], 1, "{body}");
    let finished = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", &"foo\n".repeat(2))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    assert!(finished.get("pagination").is_none(), "{finished}");
    let files = body["files"].as_array().expect("files");
    for file in files {
        assert!(file.get("totalMatchRows").is_none(), "{file}");
        assert!(file.get("returnedMatchRows").is_none(), "{file}");
    }
    let b = files.iter().find(|f| f["path"] == "b.txt").expect("b");
    let a = files.iter().find(|f| f["path"] == "a.txt").expect("a");
    assert!(a.get("pagination").is_none(), "{a}");
    assert_eq!(b["pagination"]["hasMore"], true);
    assert_eq!(b["pagination"]["totalItems"], 3);
    assert!(b["pagination"].get("pageSize").is_none(), "{b}");
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert!(next["snapshot"].is_string(), "{body}");
    // Multi-page file results still carry file pagination.
    let body = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", "foo\n")],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 1}),
            None,
        ),
    );
    assert_eq!(body["pagination"]["totalPages"], 2, "{body}");
    assert!(
        body["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string(),
        "{body}"
    );
}

// Continuations carry only the caller's window fields; the next call
// derives the view's defaults (the default context lines, the page size) the
// same way, so its snapshot still matches.
#[test]
fn continuations_derive_the_view_window_defaults() {
    let file = "foo\n".repeat(3);
    let make = |view| {
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "resultView": view, "matchPageSize": 1}),
            None,
        )
    };
    let body = search_fixture(
        &[("a.txt", &file)],
        make(LocalSearchQueryResultView::MatchOnly),
    );
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert!(next.get("contextLines").is_none(), "{next}");
    let sent = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "foo", "matchPageSize": 1, "contextLines": 0, "matchContentLength": 300}),
            None,
        ),
    );
    let next = &sent["next"]["nextMatchPage"]["query"]["queries"][0];
    assert_eq!(next["contextLines"], 0, "{next}");
    assert_eq!(next["matchContentLength"], 300, "{next}");

    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.txt"), &file).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(&make(LocalSearchQueryResultView::Paginated)),
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("first page");
    let body = serde_json::to_value(&first).expect("serialize");
    let next = body["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    // The continuation re-derives the view's defaults: neither the
    // context window nor the value width is repeated in it.
    assert!(next.get("contextLines").is_none(), "{next}");
    assert!(next.get("matchContentLength").is_none(), "{next}");
    let mut continued: LocalSearchQuery =
        serde_json::from_value(next).expect("continuation parses");
    continued.path = request.path.clone();
    execute_local_search(
        &continued,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("detailed continuation is not stale");
}

/// A malformed `include` glob is caller input, coded by the engine's typed
/// kind: a pattern that spells "glob" is not.
#[test]
fn a_bad_include_glob_is_invalid_glob() {
    let t = tempfile::tempdir().expect("tempdir");
    fs::write(t.path().join("a.rs"), "glob\n").expect("fixture");
    let (policy, security) = policy_for(t.path());
    let mut fields = serde_json::json!({"matchString": "glob", "include": ["src/{a"]});
    fields["path"] = t.path().to_string_lossy().into_owned().into();
    let error = execute_local_search(
        &ls_query(fields, None),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect_err("bad glob");
    assert_eq!(error.code, ErrorCode::InvalidGlob, "{error:?}");
}

#[cfg(unix)]
#[test]
fn a_scope_where_every_path_is_unreadable_is_an_access_error_not_empty() {
    let root = tempfile::tempdir().expect("fixture directory");
    let Some(hidden) = locked_secret(root.path()) else {
        return;
    };
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-only-here".to_string(), "regex": LocalSearchQueryRegex::Literal, "includeIgnored": true}),
        None,
    );
    let outcome =
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None);
    unlock_dir(&hidden);
    let error = outcome.expect_err("every candidate failed");
    assert_eq!(error.code, ErrorCode::FileAccessFailed);
    assert!(
        error.message.contains("Permission denied"),
        "{}",
        error.message
    );
    // The message names the count and first failure; no advice rides along.
    assert!(error.hints.is_empty(), "{:?}", error.hints);
}

#[cfg(unix)]
#[test]
fn unreadable_paths_beside_readable_misses_make_the_result_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("open.txt"), "nothing here\n").expect("fixture");
    let Some(hidden) = locked_secret(root.path()) else {
        return;
    };
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-only-here".to_string(), "regex": LocalSearchQueryRegex::Literal, "includeIgnored": true}),
        None,
    );
    let outcome =
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None);
    unlock_dir(&hidden);
    let result = outcome.expect("readable file searched");
    assert_eq!(result.status, SearchStatus::Partial);
    let body = serde_json::to_value(&result).expect("serialize");
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert_eq!(body["fileErrors"]["count"], 1, "{body}");
    // The count is stated once, as a warning; no advice.
    assert!(
        body["warnings"].to_string().contains("1 path unreadable"),
        "{body}"
    );
    crate::tools::test_support::checks::assert_absent(&body["hints"], "unreadable");
}

#[test]
fn a_nul_keeps_the_matches_before_it_and_warns() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("mixed.txt"),
        b"alpha before\0alpha after\nalpha before\n",
    )
    .expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
    );
    assert_ne!(result.status, SearchStatus::Empty);
    let body = serde_json::to_value(&result).expect("serialize");
    assert_eq!(body["files"][0]["matches"][0]["line"], 1, "{body}");
    assert_eq!(body["matchCount"], 1, "{body}");
    // The warning names the file it could not search past the NUL.
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.starts_with("binaryFileSkipped: mixed.txt was searched")),
        "{:?}",
        result.warnings
    );
}

/// A value clipped to matchContentLength carries `next.expandValues`:
/// a read of its source lines that returns it whole, one per file.
#[test]
fn clipped_values_carry_reads_that_return_them_whole() {
    let root = tempfile::tempdir().expect("fixture directory");
    let long = format!("{} alpha {}", "x".repeat(3000), "y".repeat(3000));
    fs::write(
        root.path().join("long.txt"),
        format!("short alpha\n{long}\nalpha z\n"),
    )
    .expect("fixture");
    let other = format!("alpha {}", "q".repeat(900));
    fs::write(root.path().join("other.txt"), format!("{other}\n")).expect("fixture");
    fs::write(root.path().join("plain.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha", "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    assert!(
        body["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["matches"].as_array().into_iter().flatten())
            .any(|row| row.get("originalChars").is_some()),
        "{body}"
    );
    let (reads, content) = follow_expansions(&result, &policy);
    assert_eq!(reads, 2, "{body}");
    assert!(content.contains(&long), "{content:.300}");
    assert!(content.contains(&other), "{content:.300}");
    // An unclipped file needs no read.
    assert!(!content.contains("plain"), "{content:.300}");
    let next = result.next.clone().unwrap_or_default();
    // One lead per page, one row per clipped file.
    assert_eq!(
        next.as_object()
            .into_iter()
            .flatten()
            .filter(|(name, _)| name.starts_with("expandValues"))
            .count(),
        1,
        "{next}"
    );
    let ranges = next["expandValues"]["query"]["queries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["ranges"].clone())
        .collect::<Vec<_>>();
    assert!(ranges.contains(&serde_json::json!(["2-2"])), "{ranges:?}");
    assert!(ranges.contains(&serde_json::json!(["1-1"])), "{ranges:?}");
}

/// Clipped rows with context read their whole windows; overlapping
/// windows merge, and more than ten ranges continue in a second row of the
/// same read.
#[test]
fn clipped_context_windows_merge_and_split_across_reads() {
    let root = tempfile::tempdir().expect("fixture directory");
    let mut text = String::new();
    for line in 1..=200 {
        if line % 10 == 0 {
            text.push_str(&format!("alpha {}\n", "w".repeat(400)));
        } else {
            text.push_str(&format!("line {line}\n"));
        }
    }
    fs::write(root.path().join("many.txt"), &text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha",
                "regex": LocalSearchQueryRegex::Literal, "contextLines": 1, "matchContentLength": 100}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let next = result.next.clone().unwrap_or_default();
    assert_eq!(
        next["expandValues"]["query"]["queries"][0]["ranges"][0], "9-11",
        "{next}"
    );
    assert_eq!(
        next["expandValues"]["query"]["queries"][0]["ranges"]
            .as_array()
            .map(Vec::len),
        Some(10),
        "{next}"
    );
    assert_eq!(
        next["expandValues"]["query"]["queries"][1]["ranges"],
        serde_json::json!([
            "109-111", "119-121", "129-131", "139-141", "149-151", "159-161", "169-171", "179-181",
            "189-191", "199-201"
        ]),
        "{next}"
    );
    let (_, content) = follow_expansions(&result, &policy);
    let hit = format!("alpha {}", "w".repeat(400));
    assert_eq!(content.matches(hit.as_str()).count(), 20, "{content:.300}");
    for line in (10..=190).step_by(10) {
        assert!(content.contains(&format!("line {}\n", line + 1)), "{line}");
    }
}

/// A widened page is a fresh run: on a ranked walk's later page it carries
/// neither the snapshot nor the saved ranking of the narrower page (both
/// name the old value width), so following it re-ranks instead of
/// rejecting the lead as stale.
#[test]
fn a_ranked_later_page_widens_without_its_saved_ranking() {
    let root = crate::tools::test_support::Fixture::new();
    for index in 0..4 {
        root.write(
            &format!("f{index}.txt"),
            format!("alpha {}\n", "v".repeat(300 + index)),
        );
    }
    let (policy, security) = policy_for(&root.0);
    let run = |query: &serde_json::Value| {
        let mut query = query.clone();
        query["mainGoal"] = serde_json::json!("test");
        query["reasoning"] = serde_json::json!("test");
        select_all_search(
            &serde_json::from_value(query).expect("query"),
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
        )
    };
    let first = run(&serde_json::json!({"path": root.0, "matchString": "alpha",
        "regex": "literal", "pageSize": 2, "matchContentLength": 50,
        "clasifyContext":{"classifiers":["find alpha"]}}))
    .expect("first page");
    let next = first.next.expect("next");
    let second = run(&next["nextPage"]["query"]["queries"][0]).expect("second page");
    let next = second.next.expect("next");
    let widened = &next["expandValues"]["query"]["queries"][0];
    assert_eq!(next["expandValues"]["tool"], "localSearch", "{next}");
    for cursor in ["snapshot", "selection"] {
        crate::tools::test_support::checks::assert_absent(widened, cursor);
    }
    let again = run(widened).expect("the widened page runs");
    let again = serde_json::to_value(&again).expect("serialize");
    crate::tools::test_support::checks::assert_absent(&again, "originalChars");
}

/// A caller-sized page keeps its rows at any value width: one widened
/// copy of the query returns every clipped value of the page whole.
#[test]
fn clipped_values_on_a_grid_page_widen_the_same_page() {
    let root = tempfile::tempdir().expect("fixture directory");
    for index in 0..4 {
        fs::write(
            root.path().join(format!("f{index}.txt")),
            format!("alpha {}\n", "v".repeat(300 + index)),
        )
        .expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha",
                "regex": LocalSearchQueryRegex::Literal, "pageSize": 2, "matchContentLength": 50}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    let next = result.next.clone().unwrap_or_default();
    let mut names = next
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    names.sort();
    // The first page also reads its top file (the natural next step).
    assert_eq!(names, ["expandValues", "nextPage", "read"], "{next}");
    // The snapshot names the value width: the widened page runs fresh.
    assert!(
        next["expandValues"]["query"]["queries"][0]
            .get("snapshot")
            .is_none(),
        "{next}"
    );
    let widened = &next["expandValues"]["query"]["queries"][0];
    assert_eq!(next["expandValues"]["tool"], "localSearch", "{next}");
    assert_eq!(widened["page"], 1, "{widened}");
    let longest = body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["matches"][0]["originalChars"].as_u64())
        .max()
        .expect("clipped");
    assert_eq!(widened["matchContentLength"], longest, "{widened}");
    let mut widened = widened.clone();
    widened["mainGoal"] = serde_json::json!("test");
    widened["reasoning"] = serde_json::json!("test");
    let again = execute_local_search(
        &serde_json::from_value(widened).expect("widened query"),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("widened search");
    let again = serde_json::to_value(&again).expect("serialize");
    crate::tools::test_support::checks::assert_absent(&again, "originalChars");
    let paths = |value: &serde_json::Value| {
        value["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| file["path"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&again), paths(&body), "the same rows, whole");
}

#[test]
fn widening_values_preserves_duplicate_row_order() {
    let root = tempfile::tempdir().expect("fixture");
    let first = format!("needle {} A", "a".repeat(50));
    let second = format!("needle {} B", "a".repeat(50));
    let third = format!("needle {} C", "b".repeat(50));
    fs::write(
        root.path().join("hits.txt"),
        format!("{first}\n{second}\n{third}\n{first}\n"),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    for page in 1..=4 {
        let request = ls_query(
            serde_json::json!({"path": root.path(), "matchString": "needle",
                "matchContentLength": 20, "matchPageSize": 1, "matchPage": page}),
            None,
        );
        let narrow =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("narrow page");
        let next = narrow.next.as_ref().expect("expansion");
        assert_eq!(next["expandValues"]["tool"], "localSearch");
        let widened = ls_query(next["expandValues"]["query"]["queries"][0].clone(), None);
        let whole =
            execute_local_search(&widened, &policy, &security, &NeverCancel, None, None, None)
                .expect("expanded page");
        let ids = |result: &LocalSearchResult| {
            result
                .files
                .iter()
                .flat_map(|file| {
                    file.matches
                        .iter()
                        .flatten()
                        .map(move |hit| (file.path.clone(), hit.line))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&narrow), ids(&whole), "matchPage={page}");
        let hit = &whole.files[0].matches.as_ref().expect("match rows")[0];
        assert_eq!(hit.line, page);
        assert!(!hit.clipped);
    }
}

/// A clipped multiline value hides its line span: its file is searched
/// again alone with room for the longest clipped value.
#[test]
fn clipped_multiline_values_widen_a_file_scoped_search() {
    let root = tempfile::tempdir().expect("fixture directory");
    let body = (0..60).map(|n| format!("  item{n},\n")).collect::<String>();
    fs::write(
        root.path().join("list.js"),
        format!("const list = [\n{body}];\n"),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
                "matchString": "const list = \\[[^\\]]*\\]", "regex": "rust", "multiline": "on"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body_json = serde_json::to_value(&result).expect("serialize");
    assert!(
        body_json.to_string().contains("\"originalChars\""),
        "{body_json}"
    );
    let next = result.next.clone().unwrap_or_default();
    let widened = &next["expandValues"]["query"]["queries"][0];
    assert_eq!(next["expandValues"]["tool"], "localSearch", "{next}");
    assert_eq!(widened["path"], "list.js", "{next}");
    let original = body_json["files"][0]["matches"][0]["originalChars"]
        .as_u64()
        .expect("originalChars");
    assert_eq!(widened["matchContentLength"], original, "{next}");
    for key in ["page", "matchPage", "snapshot", "pageSize"] {
        assert!(widened.get(key).is_none(), "{key}: {widened}");
    }
    let mut widened = widened.clone();
    widened["path"] = serde_json::json!(root.path().join("list.js").to_string_lossy());
    let again = execute_local_search(
        &ls_query(widened, None),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("widened search");
    let again = serde_json::to_value(&again).expect("serialize");
    crate::tools::test_support::checks::assert_absent(&again, "originalChars");
    assert!(
        again["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["matches"].as_array().into_iter().flatten())
            .any(|row| row["value"]
                .as_str()
                .is_some_and(|value| value.contains("item59"))),
        "{again}"
    );
}

/// Every file cut at a NUL after real text is a coverage gap: the
/// warning names each one, files sharing a directory grouped under it,
/// and none summarized as a count.
#[test]
fn every_text_cut_file_is_named_grouped_by_directory() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("deep/dir")).expect("fixture dir");
    for index in 0..8 {
        fs::write(
            root.path().join(format!("deep/dir/mixed{index}.txt")),
            b"alpha before\0alpha after\n",
        )
        .expect("fixture");
    }
    fs::write(root.path().join("top.txt"), b"alpha before\0alpha after\n").expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
    );
    assert!(result.is_partial);
    let warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("binaryFileSkipped:"))
        .expect("binary warning");
    let names = (0..8)
        .map(|index| format!("mixed{index}.txt"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        warning.starts_with(&format!(
            "binaryFileSkipped: top.txt, deep/dir/{{{names}}} were searched"
        )),
        "{warning}"
    );
    assert!(!warning.contains("more"), "{warning}");
}

/// Past a handful, text-cut binary files are a count by extension with a
/// listing that names every one (`binarySkipped`), not an unbounded
/// inline warning: a build tree of `.rlib`s stays a short row.
#[test]
fn many_text_cut_files_are_a_count_with_a_lossless_listing() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("deps")).expect("fixture dir");
    for index in 0..300 {
        fs::write(
            root.path()
                .join(format!("deps/libcrate_with_a_long_name_{index:03}.rlib")),
            b"alpha before\0alpha after\n",
        )
        .expect("fixture");
    }
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "alpha", "regex": "literal", "resultView": "files", "defaultExcludes": false}),
        None,
    );
    let (policy, security) = policy_for(root.path());
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("binaryFileSkipped:"))
        .expect("binary warning");
    assert!(
        warning.starts_with("binaryFileSkipped: 300 files (.rlib 300) were"),
        "{warning}"
    );
    assert!(!warning.contains("libcrate"), "{warning}");
    let listing = result
        .next
        .as_ref()
        .and_then(|next| next.get("binarySkipped"))
        .expect("listing lead");
    assert_eq!(
        listing["query"]["queries"][0]["extensions"],
        serde_json::json!(["rlib"])
    );
    assert!(warning.len() < 200, "{warning}");
}

/// Files binary from their leading bytes (a font's magic, then a NUL)
/// are outside a text search: no partial result, a
/// count grouped by extension instead of every path, and an exact
/// structureSearch continuation that lists them all.
#[test]
fn leading_nul_binaries_are_counted_with_a_listing_continuation() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("fonts/a")).expect("fixture dir");
    for index in 0..30 {
        fs::write(
            root.path()
                .join(format!("fonts/a/Font-{index}-0123456789abcdef.woff2")),
            b"wOF2\0\x01\0\0alpha",
        )
        .expect("fixture");
    }
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("fixture");
    fs::write(root.path().join("a.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    assert!(!result.is_partial, "{body}");
    assert!(!result.terminal_limit, "{body}");
    assert_eq!(result.status, SearchStatus::Success, "{body}");
    assert!(result.stats.cap_reasons.is_empty(), "{body}");
    // A row with hits states the count in stats, not as a warning.
    assert!(result.warnings.is_empty(), "{body}");
    assert_eq!(body["binarySkipped"], 31, "{body}");
    let text = body.to_string();
    assert!(!text.contains("Font-0"), "{text}");

    let listing = &body["next"]["binarySkipped"];
    assert_eq!(listing["tool"], "structureSearch", "{body}");
    let query = listing["query"]["queries"][0].clone();
    assert_eq!(query["operation"], "files", "{listing}");
    assert_eq!(
        query["extensions"],
        serde_json::json!(["woff2", "png"]),
        "{listing}"
    );
    let listed = list_structure(query, &policy, &security);
    for index in 0..30 {
        assert!(
            listed.contains(&format!("Font-{index}-0123456789abcdef.woff2")),
            "{listed}"
        );
    }
    assert!(listed.contains("logo.png"), "{listed}");
    assert!(!listed.contains("a.txt"), "{listed}");
}

#[test]
fn classification_candidates_never_receive_clipped_secrets_even_when_excluded() {
    use crate::tools::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct RecordingJudge(Arc<Mutex<Vec<Value>>>);
    impl SelectionJudge for RecordingJudge {
        fn identity(&self) -> Value {
            json!({"test":"record-safe-candidates"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            let mut recorded = self.0.lock().expect("recorded states");
            Ok(call
                .admitted
                .iter()
                .map(|&index| {
                    let state = call.candidates[index]
                        .state
                        .as_ref()
                        .expect("prepared state");
                    recorded.push(state.clone());
                    let decision = if state["path"]
                        .as_str()
                        .expect("candidate path")
                        .ends_with("keep.txt")
                    {
                        Decision::Selected
                    } else {
                        Decision::Excluded
                    };
                    (index, decision)
                })
                .collect())
        }
    }

    let root = tempfile::tempdir().expect("fixture");
    let token = format!("{}{}", "ghp_", "1234567890abcdefghijklmnopqrstuvwxyz");
    let body = format!("token: {token} end\nplain needle line\n");
    for name in ["keep.txt", "drop.txt"] {
        fs::write(root.path().join(name), &body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    for fields in [
        json!({"matchString":"ghp_[0-9a-z]{20}|needle", "regex":"rust", "resultView":"matchOnly"}),
        json!({"matchString":"token|needle", "regex":"rust", "matchContentLength":30}),
    ] {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let selection = crate::tools::test_support::selection_fake::runtime(
            RecordingJudge(recorded.clone()),
            executor.handle().clone(),
            Duration::from_secs(5),
        );
        let request = ls_query(
            json!({
                "path": root.path(),
                "clasifyContext": {"classifiers":["Does this contain needle?"]}
            }),
            Some(&ls_query(fields, None)),
        );
        let result = execute_local_search(
            &request,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            Some(&selection),
        )
        .expect("selected search");
        let recorded = recorded.lock().expect("recorded states");
        assert_eq!(
            recorded.len(),
            2,
            "selected and excluded candidates were judged"
        );
        for state in recorded.iter() {
            let text = state["matches"].to_string();
            assert!(text.contains("needle"), "clean evidence remains available");
            assert!(
                text.contains("[REDACTED"),
                "secret overlap was redacted before judgment"
            );
            for fragment in ["1234567890", "klmnopqrstuvwxyz"] {
                assert!(
                    !text.contains(fragment),
                    "provider received a secret fragment"
                );
            }
        }
        let receipt = result.selection.expect("selection receipt");
        assert_eq!(
            (
                receipt.selected_items,
                receipt.excluded_items,
                receipt.unknown_items
            ),
            (1, 1, 0)
        );
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "keep.txt");
    }
}
