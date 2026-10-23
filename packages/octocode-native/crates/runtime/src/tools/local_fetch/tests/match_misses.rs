//! A `matchString` miss: the leads it offers and the work it costs.

use super::*;

/// A matchString miss leads to a localSearch of the file's directory.
#[test]
fn match_miss_leads_to_directory_search() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": "nowhere_here"})),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let search = &next["findText"];
    assert_eq!(search["tool"], "localSearch", "{next}");
    assert_eq!(
        search["query"]["queries"][0]["matchString"], "nowhere_here",
        "{next}"
    );
    assert_eq!(
        search["query"]["queries"][0]["path"],
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
            serde_json::json!({"matchString": "needle", "ranges": ["1-2"]}),
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
        read["query"]["queries"][0]["matchString"], "needle",
        "{next}"
    );
    assert!(
        read["query"]["queries"][0].get("ranges").is_none(),
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
                "matchString": "(?<=x)nowhere",
                "regex": "pcre2",
                "caseMode": "insensitive",
                "wholeWord": true
            }),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["query"]["queries"][0];
    assert_eq!(row["matchString"], "(?<=x)nowhere", "{next}");
    assert_eq!(row["regex"], "pcre2", "{next}");
    assert_eq!(row["caseMode"], "insensitive", "{next}");
    assert_eq!(row["wholeWord"], true, "{next}");
}

/// Several literal patterns that all missed: the lead searches every one of
/// them, each escaped, so `a.b` does not widen to any character.
#[test]
fn a_match_miss_lead_searches_every_literal_pattern_escaped() {
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
    let row = &next["findText"]["query"]["queries"][0];
    assert_eq!(
        row["matchString"], "(?i:no\\.where)|(?:Elsewhere)",
        "{next}"
    );
    assert_eq!(row["regex"], "rust", "{next}");
    assert_eq!(row["caseMode"], "sensitive", "{next}");
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
