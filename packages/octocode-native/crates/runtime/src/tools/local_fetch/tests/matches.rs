use super::*;

fn selection_runtime(handle: tokio::runtime::Handle) -> crate::tools::selection::Runtime {
    use crate::tools::test_support::selection_fake;
    selection_fake::runtime(
        selection_fake::SelectAll,
        handle,
        std::time::Duration::from_secs(5),
    )
}

/// `a.txt` of `lines` numbered lines where each line in `hits` reads
/// `hit<n>`, and a query matching `hit` with one context line.
fn hit_lines(t: &Fixture, lines: usize, hits: &[usize]) -> (Paths, LocalFetchQuery) {
    let p = t.0.join("a.txt");
    let text = hits.iter().fold(numbered(lines), |text, hit| {
        text.replace(&format!("l{hit}\n"), &format!("hit{hit}\n"))
    });
    fs::write(&p, text).expect("test fixture operation should succeed");
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    req.context_lines = Some(1);
    (Paths(t.0.clone()), req)
}

#[test]
fn context_read_replays_selected_plan_and_rejects_source_drift() {
    let temp = Fixture::new();
    let path = temp.0.join("goals.txt");
    fs::write(&path, "first\nsecond\nthird\n").expect("fixture");
    let paths = Paths(temp.0.clone());
    let mut query = q(&path);
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find second"]}))
            .expect("context"),
    );
    query.length = wire_positive(1);
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let read = |query: &LocalFetchQuery| {
        execute_local_fetch(query, &paths, &Safe, &NeverCancel, None, Some(&runtime))
    };
    let first = read(&query);
    assert_eq!(
        first.selection.as_ref().map(|r| r.status),
        Some(octocode_contract_types::SelectionStatus::Complete)
    );
    let continued = first
        .next
        .expect("next")
        .continue_window
        .expect("page")
        .query;
    assert!(continued.selection.is_some());
    let second = read(&continued);
    assert_eq!(second.content.as_deref(), Some("second\n"));
    let mut switched_to_full = continued.clone();
    switched_to_full.full_content = Some(true);
    switched_to_full.unit = None;
    switched_to_full.offset = None;
    switched_to_full.length = None;
    assert_eq!(
        read(&switched_to_full).error_code.as_deref(),
        None,
        "fullContent changes output paging, not the bound extraction"
    );
    let mut changed = continued.clone();
    changed.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["a different goal"]}))
            .expect("context"),
    );
    assert_eq!(read(&changed).error_code.as_deref(), Some("staleSnapshot"));
    fs::write(&path, "changed evidence\n").expect("change source");
    assert_eq!(
        read(&continued).error_code.as_deref(),
        Some("staleSnapshot")
    );
}

#[test]
fn context_read_with_no_matches_reports_empty_selection_scope() {
    let temp = Fixture::new();
    let path = temp.0.join("empty-selection.txt");
    fs::write(&path, "ordinary text\n").expect("fixture");
    let mut query = q(&path);
    query.match_string = Some(MatchString::String("missing".parse().expect("match")));
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find missing evidence"]}))
            .expect("context"),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let result = execute_local_fetch(
        &query,
        &Paths(temp.0.clone()),
        &Safe,
        &NeverCancel,
        None,
        Some(&runtime),
    );
    let receipt = result.selection.expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Complete
    );
    assert_eq!(receipt.selected_items, 0);
    assert_eq!(receipt.total_items, 0);
}

/// A classified read refused before selection leads to the caller's own
/// read without the classifier: its window, ranges and view stay, bound to
/// the source version the refusal saw.
#[test]
fn a_refused_classified_read_keeps_its_window_in_the_unfiltered_lead() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    let text = numbered(40);
    fs::write(&path, &text).expect("fixture");
    let digest = hex::encode(sha2::Sha256::digest(text.as_bytes()));
    let paths = Paths(temp.0.clone());
    for read in [
        serde_json::json!({"unit": "bytes", "offset": 0, "length": 50}),
        serde_json::json!({"ranges": ["1-30"], "offset": 10}),
        serde_json::json!({"minify": "standard", "offset": 0, "length": 5}),
    ] {
        let mut query = qj(&path, read.clone());
        query.clasify_context = Some(
            serde_json::from_value(
                serde_json::json!({"classifiers":["Does this explain ownership?"]}),
            )
            .expect("context"),
        );
        let row = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, None);
        assert_eq!(
            row.error_code.as_deref(),
            Some("invalidInput"),
            "{read}: {row:?}"
        );
        let lead = row
            .next
            .expect("next")
            .unfiltered
            .expect("unfiltered")
            .query;
        // The row names the file as the workspace does.
        let mut expected = qj(Path::new(lead.path.as_str()), read.clone());
        expected.snapshot = digest.parse().ok();
        assert_eq!(
            serde_json::to_value(&lead).expect("json"),
            serde_json::to_value(&expected).expect("json"),
            "{read}"
        );
    }
}

#[test]
fn selection_recovery_keeps_fetch_snapshot_and_rejects_changed_source() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    fs::write(&path, "original evidence\n").expect("fixture");
    let paths = Paths(temp.0.clone());
    let mut query = q(&path);
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["Does this explain ownership?"]}))
            .expect("context"),
    );
    let first = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, None);
    assert!(first.content.as_deref().unwrap_or_default().is_empty());
    let captured = first.source_sha256.expect("captured source identity");
    let recovery = first
        .next
        .expect("next")
        .unfiltered
        .expect("unfiltered")
        .query;
    assert!(recovery.clasify_context.is_none());
    assert!(recovery.selection.is_none());
    let recovered = execute_local_fetch(&recovery, &paths, &Safe, &NeverCancel, None, None);
    assert_eq!(recovered.error_code, None);
    assert_eq!(recovered.content.as_deref(), Some("original evidence\n"));
    fs::write(&path, "changed evidence\n").expect("change source");
    let changed = execute_local_fetch(&recovery, &paths, &Safe, &NeverCancel, None, None);
    assert_eq!(changed.error_code.as_deref(), Some("staleSnapshot"));
    assert_eq!(
        recovery.snapshot.as_ref().map(|value| value.as_str()),
        Some(captured.as_str())
    );
}

#[test]
fn unknown_and_whole_selection_recovery_reject_source_drift() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    let lines = ["selected", "unknown one", "unknown two"]
        .map(|label| format!("{label} {}\n", "x".repeat(1_200)));
    let original = lines.concat();
    fs::write(&path, &original).expect("fixture");
    let paths = Paths(temp.0.clone());
    let query = qj(
        &path,
        serde_json::json!({
            "fullContent": true,
            "clasifyContext": {"classifiers": ["Find direct evidence"]}
        }),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let mut runtime = selection_runtime(executor.handle().clone());
    runtime.limits.max_candidates = 1;
    let first = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, Some(&runtime));
    let receipt = first.selection.expect("selection receipt");
    assert_eq!((receipt.selected_items, receipt.unknown_items), (1, 2));
    let captured = first.source_sha256.expect("captured source");
    let next = first.next.expect("recovery");
    let recoveries = [
        (
            next.unfiltered_unknown.expect("unknown chunks").query,
            lines[1..].concat(),
        ),
        (next.unfiltered.expect("whole view").query, original),
    ];
    for (recovery, expected) in &recoveries {
        assert!(recovery.clasify_context.is_none() && recovery.selection.is_none());
        assert_eq!(
            recovery.snapshot.as_ref().map(|snapshot| snapshot.as_str()),
            Some(captured.as_str())
        );
        let unchanged = fetch(recovery, &paths);
        assert_eq!(unchanged.error_code, None);
        assert_eq!(unchanged.content.as_deref(), Some(expected.as_str()));
    }
    let changed_source = lines
        .iter()
        .map(|line| line.replace("unknown", "changed"))
        .collect::<String>();
    fs::write(&path, changed_source).expect("change source");
    for (recovery, _) in &recoveries {
        let changed = fetch(recovery, &paths);
        assert_eq!(changed.error_code.as_deref(), Some("staleSnapshot"));
        assert!(changed.content.as_deref().unwrap_or_default().is_empty());
    }
}

#[test]
fn unread_block_and_whole_line_recovery_reject_source_drift() {
    let temp = Fixture::new();
    let paths = Paths(temp.0.clone());
    let block = format!(
        "fn work() {{\n{}}}\n",
        (0..450)
            .map(|n| format!("    let value_{n} = {n};\n"))
            .collect::<String>()
    );
    let rest = block
        .lines()
        .skip(400)
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let long_line = format!("{}needle{}\n", "a".repeat(4_000), "b".repeat(4_000));
    let cases = [
        (
            "block.rs",
            block.replace("let value_425 = 425;", "let changed_evidence = 425;"),
            block,
            serde_json::json!({"ranges": ["1-1"], "block": true, "length": 400}),
            "continueBlock",
            rest,
        ),
        (
            "long.txt",
            long_line.replacen('a', "changed_evidence", 1),
            long_line.clone(),
            serde_json::json!({"matchString": "needle"}),
            "wholeLines",
            long_line,
        ),
    ];
    for (name, changed, original, fields, kind, expected) in cases {
        let path = temp.0.join(name);
        fs::write(&path, &original).expect("fixture");
        let first = fetch(&qj(&path, fields), &paths);
        let next = serde_json::to_value(first.next.expect("unread evidence")).expect("next");
        let recovery: LocalFetchQuery =
            serde_json::from_value(next[kind]["query"]["queries"][0].clone()).expect("recovery");
        let captured = octocode_engine::digest::sha256(original.as_bytes());
        assert_eq!(
            recovery.snapshot.as_ref().map(|snapshot| snapshot.as_str()),
            Some(captured.as_str()),
            "{kind} pins the source bytes"
        );
        let unchanged = fetch(&recovery, &paths);
        assert_eq!(unchanged.error_code, None, "{kind}");
        assert_eq!(
            unchanged.content.as_deref(),
            Some(expected.as_str()),
            "{kind}"
        );
        fs::write(&path, changed).expect("change source");
        let stale = fetch(&recovery, &paths);
        assert_eq!(stale.error_code.as_deref(), Some("staleSnapshot"), "{kind}");
        assert!(
            stale.content.as_deref().unwrap_or_default().is_empty(),
            "{kind}"
        );
    }
}

#[test]
fn selection_provider_sees_workspace_relative_path_and_complete_evidence() {
    use crate::tools::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::Value;

    use std::time::Duration;

    struct PathJudge;
    impl SelectionJudge for PathJudge {
        fn identity(&self) -> Value {
            serde_json::json!({"backend":"path-test"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&ordinal| {
                    let state = call.candidates[ordinal].state.as_ref().expect("state");
                    assert_eq!(state["path"], "src/evidence.txt");
                    assert!(
                        state["content"]
                            .as_str()
                            .is_some_and(|text| text.contains("evidence"))
                    );
                    (ordinal, Decision::Selected)
                })
                .collect())
        }
    }

    let t = Fixture::new();
    let folder = t.0.join("src");
    fs::create_dir_all(&folder).expect("folder");
    let file = folder.join("evidence.txt");
    let source = (0..6)
        .map(|_| format!("{}\n", "evidence ".repeat(100)))
        .collect::<String>();
    fs::write(&file, &source).expect("fixture");
    let policy = workspace_policy(&t.0);
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let selection = crate::tools::test_support::selection_fake::runtime(
        PathJudge,
        executor.handle().clone(),
        Duration::from_secs(2),
    );
    let mut query = q(&file);
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find evidence"]}))
            .expect("context"),
    );
    let result = execute_local_fetch(&query, &policy, &Safe, &NeverCancel, None, Some(&selection));
    assert_eq!(result.error_code, None);
    assert_eq!(result.content.as_deref(), Some(source.as_str()));
}
#[test]
fn exact_line_and_continuation_union() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "one\ntwo 😀\nthree\n").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.unit = Some(WindowUnit::Lines);
    req.length = wire_positive(1);
    let mut joined = String::new();
    loop {
        let r = fetch(&req, &paths);
        assert_eq!(r.status, ReadStatus::Success);
        joined.push_str(
            r.content
                .as_deref()
                .expect("test fixture operation should succeed"),
        );
        let Some(next) = r.next.and_then(|n| n.continue_window) else {
            break;
        };
        req = next.query
    }
    assert_eq!(joined, "one\ntwo 😀\nthree\n")
}
#[test]
fn regex_line_anchors_match_every_line() {
    let t = Fixture::new();
    let p = t.0.join("Cargo.toml");
    fs::write(&p, "[package]\nversion = \"1\"\n[dep]\nversion = \"2\"\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String(
        "^version.*\"$".parse().expect("match string"),
    ));
    req.regex = Some(octocode_contract_types::ReadRegex::Rust);
    req.context_lines = Some(0);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["matchedLines"], serde_json::json!([2, 4]));
}
#[test]
fn wire_form_omits_metadata_derivable_from_emitted_fields() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "zero\none\nneedle\nthree\nfour\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    req.context_lines = Some(1);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["content"], "one\nneedle\nthree\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 4}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3]));
    assert_eq!(wire["totalLines"], 5);
    assert_eq!(wire["sourceBytes"], 27);
    assert_eq!(wire["returnedBytes"], 17);
    for redundant in [
        "contentView",
        "startLine",
        "endLine",
        "matchRanges",
        "selectedMatchCount",
        "resolvedPath",
        "pagination",
    ] {
        assert!(wire.get(redundant).is_none(), "{redundant}: {wire}");
    }
    // Explanation counters are built for every read; the contract classes
    // them `stats` extras, so a response shows them only under `extras:["stats"]`.
    let verbose = crate::response::extras::all_paths(crate::id::ToolId::LocalFetch);
    for counter in [
        "modified",
        "sourceChars",
        "sourceBytes",
        "returnedChars",
        "returnedBytes",
        "returnedLines",
    ] {
        let path = format!("results[].data.{counter}");
        assert!(verbose.contains(&path.as_str()), "{counter} is not verbose");
    }

    let mut paged = q(&p);
    paged.length = wire_positive(2);
    let wire = serde_json::to_value(fetch(&paged, &paths)).expect("serializable");
    assert_eq!(
        wire["pagination"],
        serde_json::json!({"unit":"lines","offset":0,"length":2,"hasMore":true,"nextOffset":2})
    );
    assert!(wire["next"]["continueWindow"].is_object());
    paged.offset = Some(4);
    let last = serde_json::to_value(fetch(&paged, &paths)).expect("serializable");
    assert_eq!(
        last["pagination"],
        serde_json::json!({"unit":"lines","offset":4,"length":1,"hasMore":false})
    );

    let wide = t.0.join("wide.txt");
    fs::write(&wide, "a😀\n").expect("test fixture operation should succeed");
    let wire = serde_json::to_value(fetch(&q(&wide), &paths)).expect("serializable");
    assert_eq!(wire["sourceChars"], 4);
    assert_eq!(wire["sourceBytes"], 6);
    assert_eq!(wire["returnedChars"], 4);
}
#[test]
fn match_windows_are_separated_by_omission_marker() {
    let t = Fixture::new();
    let (paths, req) = hit_lines(&t, 30, &[3, 25]);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(
        wire["content"],
        "l2\nhit3\nl4\n... [lines 5-23 not requested] ...\nl24\nhit25\nl26\n"
    );
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 4},{"line": 24, "endLine": 26}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3, 25]));
}
#[test]
fn adjacent_match_windows_have_no_marker() {
    let t = Fixture::new();
    let (paths, req) = hit_lines(&t, 10, &[3, 5]);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["content"], "l2\nhit3\nl4\nhit5\nl6\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 6}])
    );
}
#[test]
fn omission_marker_survives_redaction() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(
        &p,
        numbered(10)
            .replace("l2\n", "hit\n")
            .replace("l9\n", "hit SECRET\n"),
    )
    .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert_eq!(
        r.content.as_deref(),
        Some("hit\n... [lines 3-8 not requested] ...\nhit [REDACTED]\n")
    );
    // Redaction keeps line counts, so the anchors stay; the warning says
    // the text is not verbatim.
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 2, end: 2 },
            LineRange { start: 9, end: 9 }
        ]
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("not verbatim")),
        "{:?}",
        r.warnings
    );
}
#[test]
fn paged_match_windows_map_source_ranges_around_marker() {
    let t = Fixture::new();
    let (paths, mut req) = hit_lines(&t, 30, &[3, 25]);
    req.unit = Some(WindowUnit::Lines);
    req.length = wire_positive(4);
    let first = fetch(&req, &paths);
    assert_eq!(
        first.content.as_deref(),
        Some("l2\nhit3\nl4\n... [lines 5-23 not requested] ...\n")
    );
    assert_eq!(
        first.source_line_ranges,
        vec![LineRange { start: 2, end: 4 }]
    );
    let next = first
        .next
        .and_then(|n| n.continue_window)
        .expect("second page")
        .query;
    let second = fetch(&next, &paths);
    assert_eq!(second.content.as_deref(), Some("l24\nhit25\nl26\n"));
    assert_eq!(
        second.source_line_ranges,
        vec![LineRange { start: 24, end: 26 }]
    );
}
#[test]
fn matches_on_minified_lines_default_to_byte_windows() {
    let t = Fixture::new();
    let p = t.0.join("bundle.min.js");
    let filler = "x".repeat(3_000);
    let source = format!("{filler}needle{filler}needle{filler}\nshort needle\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    let r = fetch(&req, &paths);
    let content = r.content.expect("content");
    assert_eq!(content.matches("needle").count(), 3, "{content:?}");
    assert!(content.len() < 2_000, "{} bytes", content.len());
    for part in content.split('\n').filter(|part| !part.is_empty()) {
        assert!(
            part.starts_with("... [") || source.contains(part),
            "{part:?}"
        );
    }
    // The cut windows are not the whole lines: one read reaches them.
    let next = r.next.clone().expect("next");
    assert_eq!(
        next.whole_lines
            .as_ref()
            .map(|read| read.query.context_lines),
        Some(Some(0)),
        "{next:?}"
    );
    assert!(next.continue_window.is_none(), "{next:?}");
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 2 }],
        "anchor kept"
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w.starts_with("longMatchedLines:")),
        "{:?}",
        r.warnings
    );
    assert!(
        !r.warnings.iter().any(|w| w.contains("next.wholeLines")),
        "the warning does not restate the continuation: {:?}",
        r.warnings
    );
    // An explicit contextLines keeps whole lines.
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert!(
        r.warnings.iter().all(|w| !w.contains("contextLines")),
        "{:?}",
        r.warnings
    );
}
/// An explicit `contextLines` on a minified line still starts the match
/// window at the hit: neighbouring whole lines cannot be shown without the
/// line's kilobytes, so the window is the hit's bytes and `next.wholeLines`
/// reads the whole line. Only `contextLines:0` asks for the matched line itself,
/// paged from its start.
#[test]
fn an_explicit_context_on_a_minified_line_still_windows_the_hit() {
    let t = Fixture::new();
    let p = t.0.join("bundle.min.js");
    let filler = "x".repeat(30_000);
    fs::write(&p, format!("{filler}needle{filler}\nshort line\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    for context in [1, 5] {
        req.context_lines = Some(context);
        let r = fetch(&req, &paths);
        let content = r.content.as_deref().expect("content");
        assert!(content.len() < 1_000, "{} bytes", content.len());
        assert!(content.contains("needle"), "{content:?}");
        assert!(
            content.starts_with("... [29800 bytes omitted] ...\n"),
            "{content:?}"
        );
        let next = r.next.expect("next");
        assert!(next.whole_lines.is_some(), "{next:?}");
        assert!(next.continue_window.is_none(), "{next:?}");
    }
    req.context_lines = Some(0);
    let whole = fetch(&req, &paths);
    let content = whole.content.as_deref().expect("content");
    assert!(content.starts_with("xxxx"), "the line from its start");
    assert!(!content.contains("omitted"), "{content:.80}");
    assert!(whole.next.expect("next").continue_window.is_some());
}
#[test]
fn missing_files_in_a_sparse_checkout_say_so() {
    let t = Fixture::new();
    let git = t.0.join("clone/.git");
    fs::create_dir_all(git.join("info")).expect("fixture");
    fs::write(git.join("info/sparse-checkout"), "/*\n!/*/\n/src/\n").expect("fixture");
    fs::write(git.join("config"), "[core]\n\tsparseCheckout = true\n").expect("fixture");
    let policy = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(t.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let mut req = test_local_fetch_query();
    req.path = "clone/docs/guide.md".parse().expect("path");
    let r = fetch(&req, &policy);
    assert!(r.resource_missing, "{r:?}");
    let error = r.error.as_deref().unwrap_or_default();
    assert!(error.contains("clone/docs/guide.md"), "{error}");
    assert!(error.contains("sparse checkout"), "{error}");

    assert_eq!(r.error_code.as_deref(), Some("pathNotFound"), "{r:?}");

    // An ordinary repository keeps the plain not-found message, under
    // the not-found code the other local tools use; the recovery advice
    // is the row's hint, not a second copy in the message.
    fs::write(git.join("config"), "[core]\n\tbare = false\n").expect("fixture");
    let r = fetch(&req, &policy);
    assert_eq!(
        r.error.as_deref(),
        Some("Path does not exist: clone/docs/guide.md"),
        "{r:?}"
    );
    assert_eq!(r.error_code.as_deref(), Some("pathNotFound"), "{r:?}");
    assert!(r.resource_missing, "{r:?}");
}
#[test]
fn redacted_chunk_and_range_reads_keep_their_source_anchor() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30).replace("l12\n", "l12 SECRET\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut chunk = q(&p);
    chunk.unit = Some(WindowUnit::Lines);
    chunk.offset = Some(wire_count(10));
    chunk.length = wire_positive(5);
    let mut range = q(&p);
    local_fetch_query::set_line_span(&mut range, 11, 13);
    for (req, expected) in [(chunk, (11, 15)), (range, (11, 13))] {
        let r = fetch(&req, &paths);
        assert!(
            r.content
                .as_deref()
                .is_some_and(|c| c.contains("[REDACTED]")),
            "{r:?}"
        );
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: expected.0,
                end: expected.1
            }]
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("not verbatim")),
            "{:?}",
            r.warnings
        );
    }
}
/// Byte windows (the long-line match view) never fabricate text where
/// windows overlap.
#[test]
fn byte_windows_overlapping_do_not_duplicate_source() {
    let source = "aaa needle bbb\nline2\nline3\nline4\nccc needle ddd\nneedle x needle\n";
    let spans = super::extraction::literal_ranges(source, "needle", true);
    let (content, _) = super::extraction::byte_windows(source, spans, 4);
    for part in content.split("\n").filter(|part| !part.is_empty()) {
        assert!(
            part.starts_with("... [") || source.contains(part),
            "fabricated text {part:?} in {content:?}"
        );
    }
    assert!(content.contains("bytes omitted"), "{content:?}");
}
#[test]
fn multiline_matches_preserve_every_matched_source_line() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "before\nfirst\nsecond\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, is_regex) in [("first\nsecond", false), (r"first\nsecond", true)] {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.regex = is_regex.then_some(octocode_contract_types::ReadRegex::Rust);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(wire["content"], "first\nsecond\n", "{pattern:?}: {wire}");
        assert_eq!(wire["matchedLines"], serde_json::json!([2, 3]));
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"line": 2, "endLine": 3}])
        );
    }
}

/// An omitted caseMode is smart case, like localSearch: a
/// lowercase pattern ignores case, an uppercase letter makes it exact.
#[test]
fn match_case_defaults_to_smart_case() {
    use octocode_contract_types::ReadCaseMode;
    let t = Fixture::new();
    let p = t.0.join("a.py");
    fs::write(&p, "MERGE_SETTINGS = 1\nmerge_settings()\nMerge_Settings\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, mode, lines) in [
        ("merge_settings", None, serde_json::json!([1, 2, 3])),
        ("MERGE_SETTINGS", None, serde_json::json!([1])),
        (
            "MERGE_SETTINGS",
            Some(ReadCaseMode::Insensitive),
            serde_json::json!([1, 2, 3]),
        ),
        (
            "merge_settings",
            Some(ReadCaseMode::Sensitive),
            serde_json::json!([2]),
        ),
    ] {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.case_mode = mode;
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(wire["matchedLines"], lines, "{pattern} {mode:?}: {wire}");
    }
}

/// `regex:"pcre2"` (localSearch's engine for lookaround/backreferences)
/// matches in original byte coordinates, and a multiline match keeps every
/// source line it touches.
#[test]
fn pcre2_matches_lookaround_backreferences_and_multiline_spans() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "before\nfn alpha() {\n  beta(); beta();\n}\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let read = |pattern: &str| {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.regex = Some(ReadRegex::Pcre2);
        req.context_lines = Some(0);
        serde_json::to_value(fetch(&req, &paths)).expect("serializable")
    };
    let lookbehind = read("(?<=fn )alpha");
    assert_eq!(
        lookbehind["matchedLines"],
        serde_json::json!([2]),
        "{lookbehind}"
    );
    let backreference = read(r"(beta\(\);) \1");
    assert_eq!(
        backreference["matchedLines"],
        serde_json::json!([3]),
        "{backreference}"
    );
    for (engine, pattern) in [
        (ReadRegex::Pcre2, r"alpha\(\) \{\n\s+beta"),
        (ReadRegex::Rust, r"alpha\(\) \{\n\s+beta"),
    ] {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.regex = Some(engine);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(
            wire["content"], "fn alpha() {\n  beta(); beta();\n",
            "{engine}: {wire}"
        );
        assert_eq!(
            wire["matchedLines"],
            serde_json::json!([2, 3]),
            "{engine}: {wire}"
        );
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"line": 2, "endLine": 3}]),
            "{engine}: {wire}"
        );
    }
    // Uppercase in a regex is smart case too.
    assert_eq!(read("ALPHA")["content"], "", "no line has uppercase ALPHA");
    assert_eq!(read("alph[a]")["matchedLines"], serde_json::json!([2]));
    let invalid = read("(");
    assert_eq!(invalid["errorCode"], "invalidPattern", "{invalid}");
}

#[test]
fn byte_windows_keep_original_offsets_when_unicode_lowercase_expands() {
    for (source, pattern, expected) in [
        ("İneedle tail\n", "needle", "needle"),
        ("İNEEDLE\n", "needle", "NEEDLE"),
        ("prefix İ tail\n", "İ", "İ"),
        ("İİneedle", "needle", "needle"),
    ] {
        let sensitive = pattern.chars().any(char::is_uppercase);
        let spans = super::extraction::literal_ranges(source, pattern, sensitive);
        let (content, _) = super::extraction::byte_windows(source, spans, 0);
        // A window that starts or ends mid-line says so with an omission
        // marker; the matched bytes between them are the original span.
        let span = content
            .lines()
            .filter(|line| !line.starts_with("... ["))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(span, expected, "{source:?}: {content:?}");
        let cut_before = source.find(expected).is_some_and(|at| at > 0);
        assert_eq!(
            content.starts_with("... ["),
            cut_before,
            "{source:?}: {content:?}"
        );
    }
}

#[test]
fn byte_pages_preserve_utf8_and_use_byte_offsets() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "a😀b").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.unit = Some(WindowUnit::Bytes);
    req.length = wire_positive(2);
    let a = fetch(&req, &paths);
    assert_eq!(a.content.as_deref(), Some("a😀"));
    req = a
        .next
        .expect("test fixture operation should succeed")
        .continue_window
        .expect("test fixture operation should succeed")
        .query;
    let b = fetch(&req, &paths);
    assert_eq!(b.content.as_deref(), Some("b"));
    assert_eq!(b.returned_chars, Some(1))
}
/// matchString runs on redacted text, so a match inside a redacted value is
/// not selected. When other lines do match, the row says how many redacted
/// lines lie outside the result instead of dropping them silently.
#[test]
fn a_partial_match_names_the_redacted_lines_it_could_not_see() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "CRE one\nvalue SECRET\nlast\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("CRE".parse().expect("match string")));
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert_eq!(r.content.as_deref(), Some("CRE one\n"));
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("1 redacted line") && w.contains("matchString")),
        "{:?}",
        r.warnings
    );
}

/// A redacted line the result shows as context is not "outside" it, and
/// the count, the same on every page, is stated by the first page only.
#[test]
fn redacted_context_lines_are_not_counted_outside_the_result() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    let mut source = String::from("CRE one\nvalue SECRET\n");
    source.push_str(&"filler\n".repeat(10));
    source.push_str("other SECRET\n");
    source.push_str(&"filler\n".repeat(3));
    source.push_str("CRE two\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let outside = |result: &LocalFetchResult| {
        result
            .warnings
            .iter()
            .filter(|w| w.contains("outside this result"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("CRE".parse().expect("match string")));
    // Line 2 is context of the first hit; line 13 lies outside the result.
    req.context_lines = Some(1);
    let first = fetch(&req, &paths);
    assert_eq!(
        outside(&first),
        vec!["1 redacted line outside this result: matchString cannot see text behind a [REDACTED…] placeholder; a plain read shows where they are.".to_owned()],
        "{:?}",
        first.warnings
    );
    // Both redacted lines shown as context: nothing lies outside.
    req.context_lines = Some(4);
    assert_eq!(outside(&fetch(&req, &paths)), Vec::<String>::new());
    // A later page of the same read does not repeat the count.
    req.context_lines = Some(1);
    req.length = wire_positive(2);
    let page = fetch(&req, &paths);
    assert_eq!(outside(&page).len(), 1, "{:?}", page.warnings);
    let next = page
        .next
        .and_then(|next| next.continue_window)
        .expect("a second page")
        .query;
    let second = fetch(&next, &paths);
    assert_eq!(
        outside(&second),
        Vec::<String>::new(),
        "{:?}",
        second.warnings
    );
}

#[test]
fn ranges_matches_redaction_and_binary() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "zero\nneedle SECRET\nlast\n").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    let expected_hash = hex::encode(sha2::Sha256::digest(b"zero\nneedle SECRET\nlast\n"));
    assert_eq!(r.source_sha256.as_deref(), Some(expected_hash.as_str()));
    assert_eq!(r.content.as_deref(), Some("needle [REDACTED]\n"));
    assert_eq!(r.match_ranges, vec![LineRange { start: 2, end: 2 }]);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 2, end: 2 }],
        "line-preserving redaction keeps the source anchor"
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("not verbatim")),
        "{:?}",
        r.warnings
    );
    let bin = t.0.join("b.bin");
    fs::write(&bin, [0, 1, 2]).expect("test fixture operation should succeed");
    assert_eq!(
        fetch(&q(&bin), &paths).error_code.as_deref(),
        Some("binaryFileUnsupported")
    )
}
#[test]
fn cancellation_is_checked_around_io() {
    let t = Fixture::new();
    let p = t.0.join("a");
    fs::write(&p, "text").expect("test fixture operation should succeed");
    let r = execute_local_fetch(
        &q(&p),
        &Paths(t.0.clone()),
        &Safe,
        &Cancel(AtomicUsize::new(0)),
        None,
        None,
    );
    assert_eq!(r.error_code.as_deref(), Some("cancelled"))
}
#[test]
fn schema_rejects_unknown_fields_and_invalid_combinations() {
    assert!(
        serde_json::from_value::<LocalFetchQuery>(serde_json::json!({"path":"x","madeUp":true}))
            .is_err()
    );
    // The contract is the one validator of selector combinations.
    let combined = serde_json::json!({"path":"x","fullContent":true,"matchString":"x"});
    assert!(crate::contracts::validate_query("localFetch", combined).is_err());
}

#[test]
fn latin1_text_decodes_with_a_flag_and_long_lines_fall_back_to_bytes() {
    let t = Fixture::new();
    // "café\nnaïve" in ISO-8859-1: text, but not valid UTF-8.
    let latin1 = t.0.join("latin1.txt");
    fs::write(&latin1, b"caf\xe9\nna\xefve\n").expect("latin-1 fixture should be written");
    let paths = Paths(t.0.clone());
    let decoded = fetch(&q(&latin1), &paths);
    assert_eq!(decoded.error_code, None, "{decoded:?}");
    assert_eq!(decoded.content.as_deref(), Some("café\nnaïve\n"));
    assert!(
        decoded.warnings.iter().any(|w| w.contains("Latin-1")),
        "{:?}",
        decoded.warnings
    );
    // Mostly UTF-8 with one stray byte stays UTF-8 (lossy), also flagged.
    let stray = t.0.join("stray.txt");
    fs::write(
        &stray,
        "é \u{2014} ok\n"
            .bytes()
            .chain([0xff, b'\n'])
            .collect::<Vec<_>>(),
    )
    .expect("stray fixture should be written");
    let lossy = fetch(&q(&stray), &paths);
    assert_eq!(lossy.content.as_deref(), Some("é \u{2014} ok\n\u{fffd}\n"));
    assert!(
        lossy.warnings.iter().any(|w| w.contains("UTF-8")),
        "{:?}",
        lossy.warnings
    );

    let long = t.0.join("long.txt");
    fs::write(&long, "x".repeat(18_000)).expect("long fixture should be written");
    let long_result = fetch(&q(&long), &paths);
    let pagination = long_result.pagination.expect("long result should paginate");
    assert_eq!(pagination.unit, WindowUnit::Bytes);
    assert_eq!(pagination.length, 16_384);
    assert!(pagination.has_more);
}

/// A whole-file read and its continuations page by the row's share of the
/// window, gutters and JSON included, never by a fixed line count or the
/// old byte ceiling: every page fits and the pages rebuild the source.
#[test]
fn full_content_continuation_pages_by_the_byte_budget_not_100_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("short-lines.rs");
    let source = "let x = 1;\n".repeat(6_000);
    fs::write(&file, &source).expect("fixture should be written");
    let mut query = q(&file);
    query.full_content = Some(true);
    let mut read = String::new();
    for call in 0.. {
        assert!(call < 10, "the walk ends");
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(50_000), None);
        if call == 0 {
            assert_eq!(page.partial_reasons, vec![PartialReason::ResponseShare]);
        }
        let row = serde_json::to_value(&page).expect("row");
        assert_eq!(
            crate::response::read_share::fitted_lines(
                &row,
                &crate::response::result::query_value(&query),
                50_000
            ),
            None,
            "the page fits its share"
        );
        let pagination = page.pagination.clone().expect("paged");
        let content = page.content.clone().expect("content");
        read.push_str(&content);
        let Some(next) = page.next.and_then(|next| next.continue_window) else {
            break;
        };
        assert!(pagination.length > 1_000, "{pagination:?}");
        assert!(
            content.len() > 16_384,
            "the old byte ceiling still limits the page"
        );
        assert!(next.query.length.is_none(), "{:?}", next.query);
        query = next.query;
    }
    assert_eq!(read, source);
}

/// A whole-file read is cut at the configured response window, not at a
/// fixed size: the same file pages under a small window and returns whole
/// under a large one.
#[test]
fn full_content_is_cut_at_the_configured_response_window() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("mid.txt");
    fs::write(&file, "let value = 1;\n".repeat(1_500)).expect("fixture");
    let mut full = q(&file);
    full.full_content = Some(true);
    let read = |window| execute_local_fetch(&full, &paths, &Safe, &NeverCancel, window, None);
    let small = read(Some(10_000));
    assert_eq!(small.partial_reasons, vec![PartialReason::ResponseShare]);
    assert!(small.next.and_then(|next| next.continue_window).is_some());
    let large = read(Some(50_000));
    assert!(
        large.partial_reasons.is_empty(),
        "{:?}",
        large.partial_reasons
    );
    assert!(large.next.is_none());
    assert_eq!(large.returned_lines, Some(1_500));
}

#[test]
fn context_full_content_bounded_continuation_replays_its_selection() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("bounded-goals.txt");
    fs::write(&file, "let value = 1;\n".repeat(1_500)).expect("fixture");
    let mut full = q(&file);
    full.full_content = Some(true);
    full.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find value"]})).expect("context"),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let first = execute_local_fetch(
        &full,
        &paths,
        &Safe,
        &NeverCancel,
        Some(10_000),
        Some(&runtime),
    );
    assert_eq!(first.error_code, None);
    let next = first
        .next
        .expect("bounded next")
        .continue_window
        .expect("page")
        .query;
    assert_eq!(
        next.full_content, None,
        "fullContent is bounded by the output page"
    );
    assert!(next.selection.is_some());
    let second = execute_local_fetch(
        &next,
        &paths,
        &Safe,
        &NeverCancel,
        Some(10_000),
        Some(&runtime),
    );
    assert_eq!(second.error_code, None);
    assert_eq!(
        second.selection.as_ref().map(|receipt| receipt.status),
        Some(octocode_contract_types::SelectionStatus::Complete)
    );
}

#[test]
fn full_content_limits_return_executable_continuations() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let view_limited = t.0.join("view.txt");
    fs::write(&view_limited, "x".repeat(60_000)).expect("view fixture should be written");
    let mut compact = q(&view_limited);
    compact.full_content = Some(true);
    compact.minify = Some(MinifyMode::Standard);
    let result = execute_local_fetch(&compact, &paths, &Safe, &NeverCancel, Some(50_000), None);
    // Page 1 of the view is returned inline with the continuation.
    assert_eq!(result.error_code, None);
    assert!(
        result
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(result.partial_reasons, vec![PartialReason::ResponseShare]);
    assert!(result.next.and_then(|next| next.continue_window).is_some());

    let source_temp = Fixture::new();
    let source_limited = source_temp.0.join("source.txt");
    fs::write(&source_limited, "y".repeat(110 * 1024)).expect("source fixture should be written");
    let mut full = q(&source_limited);
    full.full_content = Some(true);
    let result = execute_local_fetch(
        &full,
        &Paths(source_temp.0.clone()),
        &Safe,
        &NeverCancel,
        Some(50_000),
        None,
    );
    assert_eq!(result.error_code, None);
    assert!(
        result
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(result.partial_reasons, vec![PartialReason::ResponseShare]);
    assert!(result.next.and_then(|next| next.continue_window).is_some());
}

#[test]
fn security_limit_is_terminal_but_offers_a_bounded_line_read() {
    struct Limited;
    impl ContentScan for Limited {
        fn sanitize(&self, _: &str, _: &Path) -> Result<(String, Vec<String>), (String, String)> {
            Err(("contentSecurityLimit".into(), "limited".into()))
        }
    }
    let t = Fixture::new();
    let p = t.0.join("large.txt");
    fs::write(&p, "one\ntwo\n").expect("security fixture should be written");
    let result = execute_local_fetch(
        &q(&p),
        &Paths(t.0.clone()),
        &Limited,
        &NeverCancel,
        None,
        None,
    );
    assert_eq!(result.error_code.as_deref(), Some("contentSecurityLimit"));
    assert_eq!(result.terminal_limit, Some(true));
    assert!(
        result
            .next
            .and_then(|next| next.read_bounded_lines)
            .is_some()
    );
}

/// `minify:"standard"` keeps the citation gutter: every kept line opens
/// with its own source line number and a tab; comments and blank lines
/// the view drops leave gaps in the numbers, never a wrong number.
#[test]
fn standard_view_lines_cite_their_source_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("a.js");
    fs::write(
            &p,
            "// header comment\n\nimport { a } from 'a';\n\n/* block\n   comment */\nexport function run(x) {\n  return a(x); // trailing\n}\n",
        )
        .expect("fixture");
    let read = |fields: serde_json::Value| fetch(&qj(&p, fields), &paths).content.expect("content");
    let content = read(serde_json::json!({"minify":"standard"}));
    let source = fs::read_to_string(&p).expect("source");
    let squash = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let mut numbers = Vec::new();
    for line in content.lines() {
        let (number, text) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("every line numbered: {content:?}"));
        let number = number.parse::<usize>().expect("number");
        let original = source.lines().nth(number - 1).expect("source line");
        assert!(
            squash(original).contains(&squash(text)),
            "{number}: {text:?} vs {original:?}"
        );
        numbers.push(number);
    }
    assert_eq!(numbers.first(), Some(&3), "{content:?}");
    assert!(numbers.contains(&8), "{content:?}");
    // A line window keeps the window's own numbers.
    let window = read(serde_json::json!({"minify":"standard","ranges": ["7-9"]}));
    assert!(window.starts_with("7\t"), "{window:?}");
    assert!(
        window.lines().last().is_some_and(|l| l.starts_with("9\t")),
        "{window:?}"
    );
}

/// A minified view pages in raw source lines, the unit of `totalLines`,
/// the gutter numbers and `ranges`: each page covers raw lines
/// `offset+1..=offset+length`, its gutter numbers lie inside that span,
/// pages are contiguous, and together they reach every raw line and show
/// every kept line exactly once.
#[test]
fn minified_views_page_in_raw_source_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("a.js");
    // A kept view over the response window is cut to a bounded first page.
    // Every page re-minifies the whole file, so the view stays near the
    // window (a quarter over, for the bytes minifying trims from each line).
    let window = 2 * crate::tools::local_fetch::LINE_PAGE_BYTES;
    let kept_line = |group: usize| format!("{}\texport const value{group} = {group};\n", group * 3);
    let (mut groups, mut kept_bytes) = (0, 0);
    while kept_bytes <= window + window / 4 {
        groups += 1;
        kept_bytes += kept_line(groups).len();
    }
    let source: String = (1..=groups)
        .map(|n| format!("// note {n}\n\nexport const value{n} = {n};\n"))
        .collect();
    fs::write(&p, &source).expect("fixture");
    let raw_total = groups * 3;
    let kept: Vec<usize> = (1..=groups).map(|n| n * 3).collect();
    for first in [
        // A line page that is no multiple of the three-line group cuts groups.
        serde_json::json!({"minify":"standard","unit":"lines","length":100}),
        serde_json::json!({"minify":"standard","fullContent":true}),
    ] {
        let mut query = qj(&p, first.clone());
        let (mut expected_offset, mut shown, mut pages) = (0, Vec::new(), 0);
        for _ in 0..1_000 {
            pages += 1;
            let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(window), None);
            assert_eq!(page.total_lines, Some(raw_total), "{first}");
            let pagination = page.pagination.clone().expect("paged");
            assert_eq!(pagination.unit, WindowUnit::Lines, "{first}");
            assert_eq!(pagination.total_lines, raw_total, "{first} {pagination:?}");
            assert_eq!(pagination.offset, expected_offset, "{first} {pagination:?}");
            let span = pagination.offset + 1..=pagination.offset + pagination.length;
            for line in page.content.as_deref().unwrap_or_default().lines() {
                let number: usize = line
                    .split_once('\t')
                    .and_then(|(number, _)| number.parse().ok())
                    .unwrap_or_else(|| panic!("numbered line: {line:?}"));
                assert!(span.contains(&number), "{first} {number} outside {span:?}");
                shown.push(number);
            }
            expected_offset = pagination.offset + pagination.length;
            match page.next.and_then(|next| next.continue_window) {
                Some(next) => {
                    assert!(pagination.has_more, "{first} {pagination:?}");
                    assert_eq!(
                        local_fetch_query::offset(&next.query),
                        Some(expected_offset),
                        "{first}"
                    );
                    query = next.query;
                }
                None => break,
            }
        }
        assert_eq!(
            expected_offset, raw_total,
            "{first}: pages reach the last raw line"
        );
        assert_eq!(shown, kept, "{first}: every kept line exactly once");
        assert!(pages > 1, "{first}: the view is one page");
    }
}

/// A source whose earlier redaction (a whole-file key block) broke the
/// parse names those lines as redacted text, never as syntax the grammar
/// lacks.
#[test]
fn a_symbols_outline_names_lines_a_redaction_left_unparseable() {
    let store = crate::cache::Store::new(crate::cache::CacheConfig::default(), None);
    let source = "pub fn a() -> u8 {\n    let key = 1;\n[REDACTED-PRIVATEKEYFRAGMENT]\n    key\n}\n\npub fn b() {}\n";
    let view = super::outline::symbols_view(
        &store,
        source,
        "k.rs",
        Path::new("/fixture/k.rs"),
        &crate::security::ContentSecurity::new(),
    );
    let warning = view.warning.clone().expect("recovered parse");
    assert!(
        warning.contains("secret-shaped text was redacted"),
        "{warning}"
    );
    assert!(!warning.contains("grammar lacks"), "{warning}");
}

/// A symbols view is kept in memory and on disk across calls, so it renders
/// from the redacted source: neither tier, nor the outline, holds a secret
/// of the file, and the view names the line its render redacted.
#[test]
fn a_kept_symbols_view_never_holds_a_secret_of_the_source() {
    let dir = tempfile::tempdir().expect("dir");
    let store = crate::cache::Store::new(
        crate::cache::CacheConfig::default(),
        Some(dir.path().to_owned()),
    );
    let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let source = format!("pub const GH: &str = \"{token}\";\n\npub fn run() -> u8 {{\n    1\n}}\n");
    let security = crate::security::ContentSecurity::new();
    let view = super::outline::symbols_view(
        &store,
        &source,
        "sec.rs",
        Path::new("/fixture/sec.rs"),
        &security,
    );
    let outline = view.outline.clone().expect("outline");
    assert!(outline.contains("[REDACTED"), "{outline}");
    assert!(!outline.contains(token), "{outline}");
    assert_eq!(view.redacted_lines, vec![(1, 1)]);
    let mut kept = 0;
    for entry in fs::read_dir(dir.path()).expect("cache dir") {
        let path = entry.expect("entry").path();
        if path
            .extension()
            .is_some_and(|extension| extension == "entry")
        {
            let text = fs::read_to_string(&path).expect("cache file");
            assert!(!text.contains("ghp_"), "{}: {text}", path.display());
            kept += 1;
        }
    }
    assert_eq!(kept, 1, "the view is on disk");
}

/// An outline page counts the placeholders its render put in, as a plain
/// page does.
#[test]
fn a_symbols_page_counts_the_secrets_its_outline_redacted() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("sec.ts");
    fs::write(
        &p,
        "export const KEY = 'SECRET';\n\nexport function run(): number {\n  return 1;\n}\n",
    )
    .expect("fixture");
    let row = fetch(&qj(&p, serde_json::json!({"minify":"symbols"})), &paths);
    let content = row.content.expect("content");
    assert!(!content.contains("SECRET"), "{content}");
    assert!(content.contains("[REDACTED]"), "{content}");
    assert!(
        row.warnings
            .iter()
            .any(|warning| warning.starts_with("redactedContent: 1 ")),
        "{:?}",
        row.warnings
    );
}

/// `minify:"symbols"` uses one gutter, `N\t`, in every language and on
/// every line, closing-brace lines included.
#[test]
fn symbols_view_uses_a_tab_gutter_everywhere() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("b.ts");
    fs::write(
            &p,
            "import { a } from 'a';\nimport { b } from 'b';\n\nexport class Box {\n  open(): void {\n    a();\n  }\n}\n\nexport function make(): Box {\n  return new Box();\n}\n",
        )
        .expect("fixture");
    let content = fetch(&qj(&p, serde_json::json!({"minify":"symbols"})), &paths)
        .content
        .expect("content");
    assert!(!content.is_empty());
    for line in content.lines() {
        let (number, _) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("tab gutter: {line:?} in {content:?}"));
        // A declaration head reads `start-end`; every other line one number.
        let mut bounds = number.splitn(2, '-');
        assert!(
            bounds.all(|bound| bound.parse::<u64>().is_ok()),
            "{line:?} in {content:?}"
        );
    }
}

/// The outline's lines are the source's own text: spaces between tokens
/// (`): boolean {`, `a: string, ...rest`) and a member's indentation stay.
#[test]
fn symbols_view_keeps_the_sources_spacing() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("s.ts");
    let source = "export function isBlank(str: string, ...rest: any[]): boolean {\n  return !str;\n}\n\nexport class Box {\n  open(a: string, b: number): void {\n    a();\n    b();\n  }\n  close(): void {\n    a();\n    b();\n  }\n}\n";
    fs::write(&p, source).expect("fixture");
    let content = fetch(&qj(&p, serde_json::json!({"minify":"symbols"})), &paths)
        .content
        .expect("content");
    for text in [
        "export function isBlank(str: string, ...rest: any[]): boolean {",
        "export class Box {",
        "  open(a: string, b: number): void {",
    ] {
        assert!(
            content.lines().any(|line| line.ends_with(text)),
            "{text:?} in {content:?}"
        );
    }
}

/// Each page names only the matched windows and lines it shows: a later
/// page never restates what an earlier page sent.
#[test]
fn match_ranges_cover_only_the_page_that_shows_them() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    let mut text = numbered(300);
    text = text
        .replace("\nl10\n", "\nl10 needle\n")
        .replace("\nl250\n", "\nl250 needle\n");
    fs::write(&p, &text).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(
        &p,
        serde_json::json!({"matchString": "needle", "contextLines": 2, "unit": "lines", "length": 5}),
    );
    let first = fetch(&req, &paths);
    assert_eq!(first.error, None, "{first:?}");
    let wire = serde_json::to_value(&first).expect("serialize");
    // The page's windows are its source lines: nothing to restate.
    assert!(wire.get("matchRanges").is_none(), "{wire}");
    assert_eq!(wire["matchedLines"], serde_json::json!([10]), "{wire}");
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("second page")
        .query;
    let second = serde_json::to_value(fetch(&next, &paths)).expect("serialize");
    assert!(
        !second.to_string().contains("\"start\":8"),
        "page 2 never restates page 1's window: {second}"
    );
    assert!(second.get("matchRanges").is_none(), "{second}");
    assert_eq!(second["matchedLines"], serde_json::json!([250]), "{second}");
}

#[test]
fn ranges_read_several_windows_in_one_row() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30)).expect("fixture");
    let paths = Paths(t.0.clone());
    // Unordered, overlapping, and past-the-end ranges merge and clamp.
    let req = qj(
        &p,
        serde_json::json!({"ranges": ["20-21", "2-3", "3-4", "29-40"]}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(r.error, None, "{r:?}");
    assert_eq!(
        r.content.as_deref(),
        Some(
            "l2\nl3\nl4\n... [lines 5-19 not requested] ...\nl20\nl21\n... [lines 22-28 not requested] ...\nl29\nl30\n"
        )
    );
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 2, end: 4 },
            LineRange { start: 20, end: 21 },
            LineRange { start: 29, end: 30 }
        ]
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("29-40")),
        "{:?}",
        r.warnings
    );
    // One range reads that span.
    let old = qj(&p, serde_json::json!({"ranges": ["2-3"]}));
    assert_eq!(fetch(&old, &paths).content.as_deref(), Some("l2\nl3\n"));
    // Ranges wholly past the end are an error, not an empty success.
    let past = qj(&p, serde_json::json!({"ranges": ["40-41"]}));
    assert!(fetch(&past, &paths).error.is_some());
    // A reversed range reads the same span and says how it was read.
    let reversed = fetch(&qj(&p, serde_json::json!({"ranges": ["3-2"]})), &paths);
    assert_eq!(reversed.content.as_deref(), Some("l2\nl3\n"));
    assert!(
        reversed
            .warnings
            .iter()
            .any(|w| w.contains("\"3-2\" is reversed")),
        "{:?}",
        reversed.warnings
    );
}

#[test]
fn block_reads_through_the_enclosing_declaration() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(
            &p,
            "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n",
        )
        .expect("fixture");
    let paths = Paths(t.0.clone());
    let ranged = qj(&p, serde_json::json!({"ranges": ["8-9"], "block": true}));
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 8, end: 11 }],
        "{r:?}"
    );
    let matched = qj(
        &p,
        serde_json::json!({"matchString": ["return a", "return 1"], "contextLines": 0, "block": true}),
    );
    let r = fetch(&matched, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 3, end: 5 },
            LineRange { start: 8, end: 11 }
        ],
        "{r:?}"
    );
    assert_eq!(r.matched_lines, vec![5, 10]);
}

/// A match window that stops inside its declaration shows a short rest
/// inline and leaves a long one unshown with no automatic reread lead:
/// `block` or `ranges` reads it when the caller needs it.
#[test]
fn a_cut_declaration_inlines_a_short_rest_and_offers_no_reread_of_a_long_one() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let small: String = (2..=10).map(|i| format!("    s{i} = {i}\n")).collect();
    let big: String = (13..=52).map(|i| format!("    b{i} = {i}\n")).collect();
    // small(): lines 1-10, needle at 3; big(): lines 12-52, needle at 14.
    fs::write(
        &p,
        format!(
            "def small():\n{}\ndef big():\n{}",
            small.replace("s3 = 3", "needle_small = 3"),
            big.replace("b14 = 14", "needle_big = 14")
        ),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let short = qj(
        &p,
        serde_json::json!({"matchString": "needle_small", "contextLines": 2}),
    );
    let r = fetch(&short, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 10 }],
        "{r:?}"
    );
    assert_eq!(r.matched_lines, vec![3]);
    assert!(r.next.is_none(), "{r:?}");
    let long = qj(
        &p,
        serde_json::json!({"matchString": "needle_big", "contextLines": 2}),
    );
    let r = fetch(&long, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 12, end: 16 }],
        "{r:?}"
    );
    // The caller chose this window: nothing of the result is unread.
    assert_eq!(r.is_partial, None, "{r:?}");
    assert!(r.next.is_none(), "no reread lead: {r:?}");
    // `block` reads the declaration the window cut.
    let block = qj(
        &p,
        serde_json::json!({"matchString": "needle_big", "contextLines": 2, "block": true}),
    );
    let r = fetch(&block, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 12, end: 52 }]);
    // Exact-line reads ask for no more.
    let exact = qj(
        &p,
        serde_json::json!({"matchString": "needle_small", "contextLines": 0}),
    );
    let r = fetch(&exact, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 3, end: 3 }]);
}

/// A `ranges` read returns exactly its lines, wherever they cut a
/// declaration, with no automatic reread lead.
#[test]
fn a_cut_ranges_read_offers_no_reread_lead() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let body: String = (2..=40).map(|i| format!("    x{i} = {i}\n")).collect();
    fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    for ranges in [vec!["1-10"], vec!["10-20"], vec!["3-5", "30-33"]] {
        let r = fetch(&qj(&p, serde_json::json!({ "ranges": ranges })), &paths);
        assert!(r.next.is_none(), "{ranges:?}: {r:?}");
    }
    // A read that covers its declaration offers nothing more.
    let whole = qj(&p, serde_json::json!({"ranges": ["1-41"]}));
    let r = fetch(&whole, &paths);
    assert!(r.next.is_none(), "{r:?}");
}

#[test]
fn a_match_list_is_a_grep_map_of_every_literal() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(
        &p,
        numbered(12)
            .replace("l4\n", "def a|b\n")
            .replace("l9\n", "x.y\n"),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    // Literal entries are literal: `|` and `.` are not regex syntax.
    let req = qj(
        &p,
        serde_json::json!({"matchString": ["a|b", "x.y", "absent"], "contextLines": 0}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(
        r.content.as_deref(),
        Some("def a|b\n... [lines 5-8 not requested] ...\nx.y\n")
    );
    assert_eq!(r.matched_lines, vec![4, 9]);
    let one = qj(
        &p,
        serde_json::json!({"matchString": "x.y", "contextLines": 0}),
    );
    assert_eq!(fetch(&one, &paths).matched_lines, vec![9]);
}

/// Only placeholders this read introduced count: literal `[REDACTED…]` text
/// in the source is verbatim, even in a file with real redactions.
#[test]
fn literal_placeholder_text_is_not_counted_as_a_redaction() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    let mut source = String::from("let shown = \"[REDACTED-literal]\";\n");
    source.push_str(&"let filler = 1;\n".repeat(2_000));
    source.push_str("let key = \"SECRET\";\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let redacted = |warnings: &[String]| {
        warnings
            .iter()
            .find(|warning| warning.starts_with("redactedContent"))
            .cloned()
    };
    let head = fetch(&qj(&p, serde_json::json!({"ranges": ["1-3"]})), &paths);
    assert_eq!(redacted(&head.warnings), None, "{:?}", head.warnings);
    let both = fetch(
        &qj(&p, serde_json::json!({"ranges": ["1-1", "2002-2002"]})),
        &paths,
    );
    let warning = redacted(&both.warnings).expect("the real redaction is named");
    assert!(warning.contains("redactedContent: 1 "), "{warning}");
    // A match read redacts the whole source first: its literal line is no
    // unmatchable redacted line, the real one is.
    let matched = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "shown", "contextLines": 0}),
        ),
        &paths,
    );
    assert_eq!(redacted(&matched.warnings), None, "{:?}", matched.warnings);
    assert!(
        matched
            .warnings
            .iter()
            .any(|warning| warning.starts_with("1 redacted line outside")),
        "{:?}",
        matched.warnings
    );
}

/// A file with a redaction keeps every miss lead: they count lines of the
/// redacted text, so they hold for any guess. The redaction is a fact
/// beside them.
#[test]
fn a_miss_in_a_redacted_file_keeps_its_leads() {
    let t = Fixture::new();
    let p = t.0.join("mod.rs");
    fs::write(
        &p,
        "mod executor;\nconst K: &str = \"SECRET\";\nmod Other;\n",
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let read = |query| {
        let row = fetch(&qj(&p, query), &paths);
        assert_eq!(row.status, ReadStatus::Empty, "{row:?}");
        assert!(
            row.hints
                .iter()
                .any(|hint| hint == super::no_match::REDACTED_MATCH_HINT),
            "{row:?}"
        );
        serde_json::to_value(row.next.as_ref().expect("next")).expect("json")
    };
    let next = read(serde_json::json!({"matchString": "executor|nothing"}));
    assert_eq!(
        next["repair"]["query"]["queries"][0]["matchString"],
        serde_json::json!(["executor", "nothing"]),
        "{next}"
    );
    let next = read(serde_json::json!({"matchString": "mod other", "caseMode": "sensitive"}));
    assert!(next.get("ignoreCase").is_some(), "{next}");
    let next = read(serde_json::json!({"matchString": "absent"}));
    assert!(next.get("findText").is_some(), "{next}");
}

/// A literal `|` miss leads to the same read with each part as its own
/// matchString, not to a search repeating the literal `|`.
#[test]
fn a_literal_bar_miss_repairs_to_a_match_list() {
    let t = Fixture::new();
    let p = t.0.join("mod.rs");
    fs::write(&p, "mod executor;\nmod other;\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let r = fetch(
        &qj(&p, serde_json::json!({"matchString": "executor|nothing"})),
        &paths,
    );
    assert_eq!(r.status, ReadStatus::Empty, "{r:?}");
    let next = serde_json::to_value(r.next.as_ref().expect("next")).expect("json");
    assert_eq!(
        next["repair"]["query"]["queries"][0]["matchString"],
        serde_json::json!(["executor", "nothing"]),
        "{next}"
    );
    assert!(next.get("findText").is_none(), "{next}");
    let repaired: LocalFetchQuery =
        serde_json::from_value(next["repair"]["query"]["queries"][0].clone()).expect("query");
    assert_eq!(fetch(&repaired, &paths).matched_lines, vec![1]);
    // Parts this file lacks too: the search looks for each part.
    let r = fetch(
        &qj(&p, serde_json::json!({"matchString": "absent|gone"})),
        &paths,
    );
    let next = serde_json::to_value(r.next.as_ref().expect("next")).expect("json");
    assert!(next.get("repair").is_none(), "{next}");
    let search = &next["findText"]["query"]["queries"][0];
    assert_eq!(search["regex"], "rust", "{next}");
    assert!(
        search["matchString"]
            .as_str()
            .is_some_and(|pattern| pattern.contains("absent") && pattern.contains(")|(")),
        "{next}"
    );
}

/// An explicit byte offset inside a character starts the page at that
/// character's first byte, says so, and loses no byte.
#[test]
fn a_mid_character_byte_offset_snaps_to_the_character_start() {
    let t = Fixture::new();
    let p = t.0.join("mb.txt");
    fs::write(&p, "abc語def\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"unit": "bytes", "offset": 4, "length": 4}),
        ),
        &paths,
    );
    assert_eq!(r.error_code, None, "{r:?}");
    assert_eq!(r.content.as_deref(), Some("語d"), "{r:?}");
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning.contains("starts at its first byte, 3")),
        "{:?}",
        r.warnings
    );
}

/// A symbols read of a data document outlines its keys; a file with no
/// outline names the specific reason.
#[test]
fn symbols_outline_data_keys_and_name_why_an_outline_is_missing() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let json = t.0.join("data.json");
    let mut body = String::from("{\n  \"name\": \"x\",\n  \"items\": {\n");
    body.push_str(&"    \"k\": 1,\n".repeat(200));
    body.push_str("    \"last\": 2\n  }\n}\n");
    fs::write(&json, &body).expect("fixture");
    let r = fetch(&qj(&json, serde_json::json!({"minify": "symbols"})), &paths);
    assert_eq!(r.content_view, Some(MinifyMode::Symbols), "{r:?}");
    assert_eq!(
        r.content.as_deref(),
        Some("2\t  \"name\": \"x\",\n3-205\t  \"items\": {\n")
    );
    let text = t.0.join("notes.txt");
    fs::write(&text, "plain\n".repeat(100)).expect("fixture");
    let r = fetch(&qj(&text, serde_json::json!({"minify": "symbols"})), &paths);
    assert_eq!(
        r.minify_fallback
            .as_ref()
            .map(|fallback| fallback.reason.as_str()),
        Some("unsupported-language"),
        "{r:?}"
    );
}

/// A symbols outline from a recovered parse (Flow types) says which lines
/// the parser skipped, as a block read does.
#[test]
fn a_symbols_outline_from_a_recovered_parse_names_the_skipped_lines() {
    let t = Fixture::new();
    let p = t.0.join("Hooks.js");
    let mut flow = String::from("// @flow\ntype Dispatch<A> = A => void;\n");
    for i in 0..20 {
        flow.push_str(&format!(
            "export function useState{i}<S>(init: S): [S, Dispatch<S>] {{\n  const value = init;\n  const other = value;\n  return [other, () => {{}}];\n}}\n"
        ));
    }
    fs::write(&p, &flow).expect("fixture");
    let r = fetch(
        &qj(&p, serde_json::json!({"minify": "symbols"})),
        &Paths(t.0.clone()),
    );
    assert_eq!(r.content_view, Some(MinifyMode::Symbols), "{r:?}");
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning.starts_with("The parser could not parse lines")),
        "{:?}",
        r.warnings
    );
    // A later outline page does not repeat it.
    let later = fetch(
        &qj(
            &p,
            serde_json::json!({"minify": "symbols", "offset": 5, "length": 5}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(later.content_view, Some(MinifyMode::Symbols), "{later:?}");
    assert!(
        !later
            .warnings
            .iter()
            .any(|warning| warning.starts_with("The parser could not parse lines")),
        "{:?}",
        later.warnings
    );
}

/// The pages of one large file version decode and key-redact it once; an
/// edited version is prepared again.
#[test]
fn a_large_source_is_prepared_once_per_version() {
    struct CountingKeys(AtomicUsize);
    impl ContentScan for CountingKeys {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((text.to_owned(), vec![]))
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            self.0.fetch_add(1, Ordering::Relaxed);
            crate::security::redact_private_key_blocks(content)
        }
    }
    let t = Fixture::new();
    let p = t.0.join("large.txt");
    let line = format!("{}\n", "x".repeat(79));
    fs::write(&p, line.repeat(4_000)).expect("fixture");
    let scan = CountingKeys(AtomicUsize::new(0));
    let read = |req: &LocalFetchQuery| {
        execute_local_fetch(req, &Paths(t.0.clone()), &scan, &NeverCancel, None, None)
    };
    let first = read(&q(&p));
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("a second page")
        .query;
    let second = read(&next);
    assert_eq!(second.status, ReadStatus::Success, "{second:?}");
    assert_eq!(
        scan.0.load(Ordering::Relaxed),
        1,
        "one preparation for both pages"
    );
    fs::write(&p, line.repeat(4_001)).expect("edit");
    assert_eq!(read(&q(&p)).total_lines, Some(4_001));
    assert_eq!(
        scan.0.load(Ordering::Relaxed),
        2,
        "a new version is prepared again"
    );
}

/// Rows of one batch reading the same large version at once prepare it
/// once: the rest wait for the first preparation and share it, whether they
/// read the same window or different ones.
#[test]
fn concurrent_rows_share_one_preparation() {
    struct SlowKeys(AtomicUsize);
    impl ContentScan for SlowKeys {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((text.to_owned(), vec![]))
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            self.0.fetch_add(1, Ordering::Relaxed);
            // Long enough that every row arrives while the first prepares.
            std::thread::sleep(std::time::Duration::from_millis(200));
            crate::security::redact_private_key_blocks(content)
        }
    }
    let t = Fixture::new();
    let p = t.0.join("shared.txt");
    let line = format!("{}\n", "x".repeat(79));
    fs::write(&p, line.repeat(4_000)).expect("fixture");
    let scan = SlowKeys(AtomicUsize::new(0));
    let rows = [
        q(&p),
        q(&p),
        qj(&p, serde_json::json!({"ranges": ["10-20"]})),
        qj(
            &p,
            serde_json::json!({"matchString": "xx", "contextLines": 0}),
        ),
    ];
    let start = std::sync::Barrier::new(rows.len());
    let results = std::thread::scope(|scope| {
        let workers = rows
            .iter()
            .map(|row| {
                let (scan, start, paths) = (&scan, &start, Paths(t.0.clone()));
                scope.spawn(move || {
                    start.wait();
                    execute_local_fetch(row, &paths, scan, &NeverCancel, None, None)
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("row"))
            .collect::<Vec<_>>()
    });
    assert!(
        results.iter().all(|result| result.error.is_none()),
        "{results:?}"
    );
    assert_eq!(scan.0.load(Ordering::Relaxed), 1, "one shared preparation");
}

/// A leading UTF-8 BOM is not text of line 1: a line-anchored pattern
/// matches it there and line 1 shows without it, as localSearch does, in
/// every dialect and in a plain read; the source size stays the file's.
#[test]
fn a_leading_bom_is_not_text_of_line_one() {
    let t = Fixture::new();
    let p = t.0.join("bom.txt");
    fs::write(&p, "\u{feff}alpha\nbeta alpha\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for regex in ["rust", "pcre2"] {
        let r = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": "^alpha", "regex": regex, "contextLines": 0}),
            ),
            &paths,
        );
        assert_eq!(r.matched_lines, vec![1], "{regex}: {r:?}");
        assert_eq!(r.content.as_deref(), Some("alpha\n"), "{regex}");
    }
    let plain = fetch(&qj(&p, serde_json::json!({"fullContent": true})), &paths);
    assert_eq!(
        plain.content.as_deref(),
        Some("alpha\nbeta alpha\n"),
        "{plain:?}"
    );
    assert_eq!(plain.source_bytes, Some(20), "{plain:?}");
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "alpha", "wholeWord": true, "contextLines": 0}),
        ),
        &paths,
    );
    assert_eq!(r.matched_lines, vec![1, 2], "{r:?}");
}

/// A match in a large source keeps its plain context windows: inlining a
/// cut declaration's few lines never costs a whole-file declarations parse.
#[test]
fn a_large_source_match_does_not_parse_to_inline_a_cut_declaration() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let small: String = (2..=10).map(|i| format!("    s{i} = {i}\n")).collect();
    let filler = "# filler line that only pads the module out\n".repeat(7_000);
    fs::write(
        &p,
        format!(
            "def small():\n{}{filler}",
            small.replace("s3 = 3", "needle_small = 3")
        ),
    )
    .expect("fixture");
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "needle_small", "contextLines": 2}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 5 }],
        "{r:?}"
    );
}

/// A kept symbols view is served without scanning its source again.
#[test]
fn a_kept_symbols_view_is_served_without_a_rescan() {
    struct Counting(AtomicUsize);
    impl ContentScan for Counting {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok((text.to_owned(), vec![]))
        }
    }
    let store = crate::cache::Store::new(crate::cache::CacheConfig::default(), None);
    let scan = Counting(AtomicUsize::new(0));
    let source = "pub fn run() -> u8 {\n    1\n}\n";
    let view = |scan: &Counting| {
        super::outline::symbols_view(&store, source, "run.rs", Path::new("/fixture/run.rs"), scan)
    };
    let first = view(&scan);
    let scans = scan.0.load(Ordering::SeqCst);
    assert!(scans > 0);
    let again = view(&scan);
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(scan.0.load(Ordering::SeqCst), scans, "a hit scans nothing");
}

/// A miss that already matched every pattern ignoring case does not run
/// the same patterns again to count case-insensitive hits.
#[test]
fn an_insensitive_miss_runs_its_patterns_once() {
    use crate::tools::local_fetch::extraction::RUST_REGEX_RUNS;
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "alpha\nbeta\n").expect("fixture");
    let read = |case: &str| {
        RUST_REGEX_RUNS.with(|runs| runs.set(0));
        let r = execute_local_fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": "gamma", "regex": "rust", "caseMode": case}),
            ),
            &Paths(t.0.clone()),
            &Safe,
            &NeverCancel,
            None,
            None,
        );
        assert_eq!(r.status, ReadStatus::Empty, "{r:?}");
        RUST_REGEX_RUNS.with(std::cell::Cell::get)
    };
    assert_eq!(read("insensitive"), 1);
    assert_eq!(read("sensitive"), 2);
}

/// A classified read its input alone rules out is refused before the
/// source is decoded or redacted, its lead bound to the version it saw.
#[test]
fn a_classified_read_with_an_unsupported_view_is_refused_before_any_scan() {
    struct Counting(AtomicUsize);
    impl ContentScan for Counting {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok((text.to_owned(), vec![]))
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            self.0.fetch_add(1, Ordering::SeqCst);
            (content.to_owned(), false)
        }
    }
    let t = Fixture::new();
    let p = t.0.join("run.rs");
    fs::write(&p, "pub fn run() {}\n").expect("fixture");
    for fields in [
        serde_json::json!({"minify": "standard"}),
        serde_json::json!({"unit": "bytes"}),
        serde_json::json!({"offset": 3}),
    ] {
        let mut fields = fields;
        fields["clasifyContext"] = serde_json::json!({"classifiers": ["find run"]});
        let scan = Counting(AtomicUsize::new(0));
        let r = execute_local_fetch(
            &qj(&p, fields.clone()),
            &Paths(t.0.clone()),
            &scan,
            &NeverCancel,
            None,
            None,
        );
        assert_eq!(
            r.error_code.as_deref(),
            Some("invalidInput"),
            "{fields}: {r:?}"
        );
        assert!(r.source_sha256.is_some());
        assert_eq!(scan.0.into_inner(), 0, "{fields}");
    }
}

/// `minify:"standard"` pages in the lines a raw read of the same selection
/// pages in: the extracted lines of a `ranges` read, as the source lines
/// of a whole-file read.
#[test]
fn a_standard_view_of_ranges_pages_in_the_extracted_lines() {
    let t = Fixture::new();
    let p = t.0.join("m.rs");
    let text: String = (1..=30)
        .map(|line| {
            if line % 2 == 0 {
                format!("// comment {line}\n")
            } else {
                format!("const V{line}: u8 = {line};\n")
            }
        })
        .collect();
    fs::write(&p, text).expect("fixture");
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"ranges": ["13-20"], "minify": "standard", "offset": 2, "length": 3}),
        ),
        &Paths(t.0.clone()),
    );
    let page = r.pagination.as_ref().expect("pagination");
    assert_eq!((page.offset, page.total_lines), (2, 8), "{r:?}");
    let first = r
        .content
        .as_deref()
        .and_then(|content| content.split('\t').next())
        .and_then(|line| line.parse::<usize>().ok())
        .expect("gutter");
    assert_eq!(first, 15, "{r:?}");
}

/// A NUL byte past the binary sniff's head sample makes the file binary.
#[test]
fn a_late_nul_byte_makes_an_in_memory_source_binary() {
    let t = Fixture::new();
    let p = t.0.join("late.txt");
    let mut bytes = "text line\n".repeat(1_000).into_bytes();
    bytes.extend_from_slice(b"\0\x01\x02tail\n");
    fs::write(&p, bytes).expect("fixture");
    let r = fetch(&q(&p), &Paths(t.0.clone()));
    assert_eq!(
        r.error_code.as_deref(),
        Some("binaryFileUnsupported"),
        "{r:?}"
    );
}
