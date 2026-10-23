//! Regex dialects, the match limit, and invalid patterns.

use super::*;

/// `regex:"rust"` is the Rust regex engine, the one `localSearch` runs: what it
/// refuses (backreferences, lookaround) is `invalidPattern` with the parser's
/// reason, and `next.repair` retries the same read as `regex:"pcre2"`, the
/// engine that owns them. It never needs a worker or an ECMAScript fallback.
#[test]
fn rust_regex_refuses_backreferences_and_lookaround_and_repairs_to_pcre2() {
    let t = Fixture::new();
    let p = t.0.join("regex.txt");
    fs::write(&p, "needle one\nhello aa\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, reason) in [
        (r"(\w)\1", "backreferences are not supported"),
        ("(?<=needle )one", "look-around"),
    ] {
        let result = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": pattern, "regex": "rust"}),
            ),
            &paths,
        );
        assert_eq!(
            result.error_code,
            Some(ErrorCode::InvalidPattern),
            "{pattern}: {result:?}"
        );
        let error = result.error.clone().unwrap_or_default();
        assert!(
            error.starts_with(&format!(
                "Invalid regex pattern: matchString \"{}\": ",
                pattern.replace('\\', "\\\\")
            )) && error.contains(reason),
            "{error}"
        );
        let repaired = result
            .next
            .as_ref()
            .and_then(|next| next.repair.as_ref())
            .expect("repair lead")
            .query
            .clone();
        assert_eq!(
            repaired.regex,
            Some(octocode_contract_types::ReadRegex::Pcre2)
        );
        assert_eq!(
            local_fetch_query::match_strings(&repaired),
            vec![pattern],
            "kept as written"
        );
        let fixed = fetch(&repaired, &paths);
        assert_eq!(fixed.error_code, None, "{pattern}: {fixed:?}");
        assert!(fixed.content.is_some_and(|c| !c.is_empty()), "{pattern}");
    }
}

/// The verdict is the pattern's, never the file's size: a pattern that
/// compiles matches a file past 64 KiB, and one that does not fails there
/// with the very error it gives on a small file.
#[test]
fn rust_regex_verdict_does_not_depend_on_file_size() {
    let t = Fixture::new();
    let small = t.0.join("small.txt");
    let big = t.0.join("big.txt");
    let filler = "word xx yy\n";
    fs::write(&small, "tail aa\n").expect("fixture");
    fs::write(&big, format!("{}tail aa\n", filler.repeat(7_000))).expect("fixture");
    assert!(fs::metadata(&big).expect("meta").len() > 64 * 1024);
    let paths = Paths(t.0.clone());
    let run = |file: &Path, pattern: &str| {
        fetch(
            &qj(
                file,
                serde_json::json!({"matchString": pattern, "regex": "rust", "contextLines": 0}),
            ),
            &paths,
        )
    };
    for file in [&small, &big] {
        let found = run(file, "t[a]il");
        assert_eq!(found.error_code, None, "{found:?}");
        assert!(found.content.is_some_and(|c| c.contains("tail aa")));
    }
    let on_small = run(&small, r"(\w)\1");
    let on_big = run(&big, r"(\w)\1");
    assert_eq!(on_small.error_code, Some(ErrorCode::InvalidPattern));
    assert_eq!(on_big.error, on_small.error);
}

/// A regex read with more matches than the old 10,000 in-process cap
/// selects every matching line (never a silent prefix of them).
#[test]
fn regex_read_selects_every_match_past_ten_thousand() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("many.txt");
    let source: String = (1..=12_000).map(|i| format!("hit {i}\n")).collect();
    fs::write(&p, &source).expect("fixture");
    for engine in [ReadRegex::Rust, ReadRegex::Pcre2] {
        let mut request = q(&p);
        request.match_string = Some(MatchString::String(
            "hit [0-9]+".parse().expect("match string"),
        ));
        request.regex = Some(engine);
        request.context_lines = Some(0);
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.selected_match_count, Some(12_000), "{engine:?}");
        assert!(
            result.warnings.iter().all(|w| !w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
    }
}

/// A regex read that reaches the in-process match limit discloses it:
/// isPartial, a warning naming the limit, and a localSearch lead that pages
/// every match of the same pattern in the same file.
#[test]
fn regex_read_past_the_match_limit_is_partial_with_a_search_lead() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("dense.txt");
    let mut source = String::new();
    for _ in 0..(crate::tools::source_view::extraction::MAX_REGEX_MATCHES / 1_000 + 1) {
        source.push_str(&"x".repeat(1_000));
        source.push('\n');
    }
    fs::write(&p, &source).expect("fixture");
    for engine in [ReadRegex::Rust, ReadRegex::Pcre2] {
        let mut request = q(&p);
        request.match_string = Some(MatchString::String("x".parse().expect("match string")));
        request.regex = Some(engine);
        request.context_lines = Some(0);
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.is_partial, Some(true), "{engine:?}");
        assert_eq!(
            result.partial_reasons,
            vec![PartialReason::RegexMatchLimit],
            "{engine:?}"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
        let lead = result
            .next
            .as_ref()
            .and_then(|next| next.text_search.clone())
            .expect("findText lead");
        assert_eq!(lead["tool"], "localSearch", "{lead}");
        let row = &lead["query"]["queries"][0];
        assert_eq!(row["matchString"], "x", "{lead}");
        assert_eq!(row["path"], p.to_string_lossy().as_ref(), "{lead}");
    }
}

/// When the read pages, the tool emits the findText lead beside the
/// page continuation (the response moves it to `hints.findText`).
#[test]
fn paged_regex_read_past_the_match_limit_keeps_its_search_lead() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("hits.txt");
    let source: String = (1..=crate::tools::source_view::extraction::MAX_REGEX_MATCHES + 5)
        .map(|n| format!("hit {n}\n"))
        .collect();
    fs::write(&p, &source).expect("fixture");
    let mut request = q(&p);
    request.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    request.regex = Some(ReadRegex::Rust);
    let result = execute_local_fetch(
        &request,
        &Paths(t.0.clone()),
        &Safe,
        &NeverCancel,
        None,
        None,
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("match limit")),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref().expect("next");
    assert!(next.continue_window.is_some(), "the read pages: {next:?}");
    let lead = next
        .text_search
        .as_ref()
        .expect("findText lead on a paged read");
    assert_eq!(lead["tool"], "localSearch", "{lead}");
}

/// A list item that does not compile as a regex is named by index and text
/// (`"def post("` in a `regex:"rust"` list), on both regex engines.
#[test]
fn invalid_regex_items_are_named_by_index() {
    let t = Fixture::new();
    let p = t.0.join("a.py");
    fs::write(&p, "def post(self):\n    pass\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for engine in ["rust", "pcre2"] {
        let list = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": ["pass", "def post("], "regex": engine}),
            ),
            &paths,
        );
        assert_eq!(list.error_code, Some(ErrorCode::InvalidPattern), "{list:?}");
        // The repair keeps the item that compiles and escapes the one that
        // does not, in the same engine.
        let repair = &list.next.as_ref().expect("repair lead").repair;
        let repaired = repair.as_ref().expect("repair").query.clone();
        assert_eq!(
            local_fetch_query::match_strings(&repaired),
            vec!["pass", "def post\\("],
            "{engine}"
        );
        let fixed = fetch(&repaired, &paths);
        assert_eq!(fixed.error_code, None, "{engine}: {fixed:?}");
        let error = list.error.unwrap_or_default();
        assert!(
            error.starts_with("Invalid regex pattern: matchString[1] \"def post(\": "),
            "{engine}: {error}"
        );
        let one = fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": "def post(", "regex": engine}),
            ),
            &paths,
        );
        let error = one.error.unwrap_or_default();
        assert!(
            error.starts_with("Invalid regex pattern: matchString \"def post(\": "),
            "{engine}: {error}"
        );
    }
    assert_eq!(
        crate::id::ToolId::LocalFetch
            .output()
            .error_hint(ErrorCode::InvalidPattern, &serde_json::Value::Null),
        Some(crate::response::output::NO_HINT)
    );
}

/// `contextLines` widens matchString hits; a read without matchString is
/// refused by the contract, the rule's one owner, before any file is read.
#[test]
fn context_lines_without_match_string_is_refused_by_the_contract() {
    let refused = crate::contracts::prepare_many_and_validate(
        "localFetch",
        serde_json::json!({"queries":[{"path":"a.rs","ranges":["1-1"],"contextLines":5}]}),
    )
    .expect_err("contextLines without matchString");
    assert!(
        format!("{refused:?}").contains("contextLines"),
        "{refused:?}"
    );
}
