//! A `matchString` miss: the leads it offers and the work it costs.

use super::*;

/// A matchString miss leads to a localSearch of the file's directory.
#[test]
fn match_miss_leads_to_directory_search() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": ["nowhere_here"]})),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let search = &next["findText"];
    assert_eq!(search["tool"], "localSearch", "{next}");
    assert_eq!(
        search["input"]["queries"][0]["matchString"],
        serde_json::json!(["nowhere_here"]),
        "{next}"
    );
    assert_eq!(
        search["input"]["queries"][0]["path"],
        t.0.to_string_lossy().as_ref(),
        "{next}"
    );
}

/// A `ranges` match read that missed while the file has hits outside its
/// ranges says so and leads to the same read without `ranges`, not to a
/// search of the directory.
#[test]
fn a_ranged_miss_with_hits_elsewhere_leads_to_the_unranged_read() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\nfn b() {}\nlet needle = 1;\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["needle"], "ranges":[{"startLine":1,"endLine":2}]}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(result.selected_match_count, Some(0), "{result:?}");
    assert_eq!(
        result.warnings,
        vec!["1 line matches outside the requested ranges.".to_owned()]
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let read = &next["read"];
    assert_eq!(read["tool"], "localFetch", "{next}");
    assert_eq!(
        read["input"]["queries"][0]["matchString"],
        serde_json::json!(["needle"]),
        "{next}"
    );
    assert!(
        read["input"]["queries"][0].get("ranges").is_none(),
        "{next}"
    );
    assert!(next.get("findText").is_none(), "{next}");
}

/// The miss lead searches the directory for exactly what the read looked
/// for: the regex dialect, the case rule and `wholeWord` carry over, so the
/// lead never matches more than the read would have.
#[test]
fn a_match_miss_lead_keeps_the_reads_dialect_case_and_whole_word() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({
                "matchString": ["(?<=x)nowhere"],
                "regexEngine": "pcre2",
                "caseMode": "insensitive",
                "wholeWord": true
            }),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["input"]["queries"][0];
    assert_eq!(
        row["matchString"],
        serde_json::json!(["(?<=x)nowhere"]),
        "{next}"
    );
    assert_eq!(row["regexEngine"], "pcre2", "{next}");
    assert_eq!(row["caseMode"], "insensitive", "{next}");
    assert_eq!(row["wholeWord"], true, "{next}");
}

/// Several literal patterns that all missed: the lead searches every one of
/// them with the same literal and case semantics.
#[test]
fn a_match_miss_lead_preserves_literal_patterns_and_smart_case() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["no.where", "Elsewhere"]}),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["input"]["queries"][0];
    assert_eq!(
        row["matchString"],
        serde_json::json!(["no.where", "Elsewhere"]),
        "{next}"
    );
    let row = crate::tools::test_support::opened(crate::id::ToolId::LocalSearch, row);
    assert_eq!(row["regexEngine"], "literal", "{next}");
    assert_eq!(row["caseMode"], "smart", "{next}");
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
                .any(|hint| hint == crate::tools::source_view::no_match::REDACTED_MATCH_HINT),
            "{row:?}"
        );
        serde_json::to_value(row.next.as_ref().expect("next")).expect("json")
    };
    let next = read(serde_json::json!({"matchString": ["executor|nothing"]}));
    assert_eq!(
        next["findText"]["input"]["queries"][0]["matchString"],
        serde_json::json!(["executor|nothing"]),
        "{next}"
    );
    let next = read(serde_json::json!({"matchString": ["mod other"], "caseMode": "sensitive"}));
    assert!(next.get("ignoreCase").is_some(), "{next}");
    let next = read(serde_json::json!({"matchString": ["absent"]}));
    assert!(next.get("findText").is_some(), "{next}");
}

#[test]
fn literal_bars_preserve_the_exact_match_in_related_searches() {
    let root = Fixture::new();
    let file = root.0.join("mod.rs");
    fs::write(&file, "mod executor;\nmod other;\n").expect("source");
    let paths = Paths(root.0.clone());
    for pattern in ["executor|nothing", "absent|gone"] {
        let result = fetch(
            &qj(&file, serde_json::json!({"matchString":[pattern]})),
            &paths,
        );
        assert_eq!(result.status, ReadStatus::Empty, "{result:?}");
        let calls = serde_json::to_value(result.next.as_ref().expect("search")).expect("calls");
        assert!(
            calls.get("repair").is_none(),
            "literal syntax must not be reinterpreted: {calls}"
        );
        let query = &calls["findText"]["input"]["queries"][0];
        assert_eq!(
            query["matchString"],
            serde_json::json!([pattern]),
            "{calls}"
        );
        let query = crate::tools::test_support::opened(crate::id::ToolId::LocalSearch, query);
        assert_eq!(query["regexEngine"], "literal", "{calls}");
    }
}

/// A miss that already matched every pattern ignoring case does not run
/// the same patterns again to count case-insensitive hits.
#[test]
fn an_insensitive_miss_runs_its_patterns_once() {
    use crate::tools::source_view::extraction::RUST_REGEX_RUNS;
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "alpha\nbeta\n").expect("fixture");
    let read = |case: &str| {
        RUST_REGEX_RUNS.with(|runs| runs.set(0));
        let r = execute_local_fetch(
            &qj(
                &p,
                serde_json::json!({"matchString": ["gamma"], "regexEngine": "rust", "caseMode": case}),
            ),
            &Paths(t.0.clone()),
            &Safe,
            &NeverCancel,
            None,
            None,
            &Default::default(),
        );
        assert_eq!(r.status, ReadStatus::Empty, "{r:?}");
        RUST_REGEX_RUNS.with(std::cell::Cell::get)
    };
    assert_eq!(read("insensitive"), 1);
    assert_eq!(read("sensitive"), 2);
}
