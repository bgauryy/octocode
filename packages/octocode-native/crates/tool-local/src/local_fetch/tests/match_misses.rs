//! A `matchString` miss: the leads it offers and the work it costs.

use super::*;
use octocode_test_support::json_fixture;

/// A matchString miss in a file with no outline leads to a localSearch of
/// the file's directory: its one call, beside its one hint.
#[test]
fn match_miss_leads_to_directory_search() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "plain notes\n").expect("fixture");
    let result = fetch(
        &qj(&p, json_fixture(r#"{"matchString": ["nowhere_here"]}"#)),
        &Paths(t.0.clone()),
    );
    assert_eq!(result.hints.len(), 1, "{result:?}");
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    assert_eq!(
        octocode_runtime_base::test_support::billing::offered_calls(&next).len(),
        1,
        "{next}"
    );
    let search = &next["findText"];
    assert_eq!(search["tool"], "localSearch", "{next}");
    assert_eq!(
        search["input"]["queries"][0]["matchString"],
        json_fixture(r#"["nowhere_here"]"#),
        "{next}"
    );
    // The file is named as its row names it, relative to the root, so its
    // directory is the root: the walk's default path, which a call states
    // or leaves out.
    let searched = &search["input"]["queries"][0]["path"];
    assert!(searched.is_null() || searched == ".", "{next}");
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
            json_fixture(
                r#"{
                    "matchString": ["needle"],
                    "ranges": [{"startLine": 1, "endLine": 2}]
                }"#,
            ),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(result.selected_match_count, Some(0), "{result:?}");
    assert!(result.warnings.is_empty(), "{result:?}");
    let [hint] = result.hints.as_slice() else {
        panic!("one hint: {result:?}")
    };
    assert!(hint.contains("1 line matches outside"), "{hint}");
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let read = &next["read"];
    assert_eq!(read["tool"], "localFetch", "{next}");
    assert_eq!(
        read["input"]["queries"][0]["matchString"],
        json_fixture(r#"["needle"]"#),
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
    let p = t.0.join("a.txt");
    fs::write(&p, "plain notes\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            json_fixture(
                r#"{
                    "matchString": ["(?<=x)nowhere"],
                    "regexEngine": "pcre2",
                    "caseMode": "insensitive",
                    "wholeWord": true
                }"#,
            ),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["input"]["queries"][0];
    assert_eq!(
        row["matchString"],
        json_fixture(r#"["(?<=x)nowhere"]"#),
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
    let p = t.0.join("a.txt");
    fs::write(&p, "plain notes\n").expect("fixture");
    let result = fetch(
        &qj(
            &p,
            json_fixture(r#"{"matchString": ["no.where", "Elsewhere"]}"#),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let row = &next["findText"]["input"]["queries"][0];
    assert_eq!(
        row["matchString"],
        json_fixture(r#"["no.where", "Elsewhere"]"#),
        "{next}"
    );
    let row = octocode_runtime_base::test_support::opened(
        octocode_runtime_base::id::ToolId::LocalSearch,
        row,
    );
    assert_eq!(row["regexEngine"], "literal", "{next}");
    assert_eq!(row["caseMode"], "smart", "{next}");
}

/// A file with a redaction keeps its miss recovery: one hint and one call,
/// which count lines of the redacted text, so they hold for any guess. The
/// redaction is a fact beside them.
#[test]
fn a_miss_in_a_redacted_file_keeps_its_leads() {
    let t = Fixture::new();
    let p = t.0.join("mod.txt");
    fs::write(
        &p,
        "mod executor;\nconst K: &str = \"SECRET\";\nmod Other;\n",
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let read = |query| {
        let row = fetch(&qj(&p, query), &paths);
        assert_eq!(row.status, ReadStatus::Empty, "{row:?}");
        assert_eq!(row.hints.len(), 1, "{row:?}");
        serde_json::to_value(row.next.as_ref().expect("next")).expect("json")
    };
    let next = read(json_fixture(r#"{"matchString": ["executor|nothing"]}"#));
    assert_eq!(
        next["findText"]["input"]["queries"][0]["matchString"],
        json_fixture(r#"["executor|nothing"]"#),
        "{next}"
    );
    let next = read(json_fixture(
        r#"{"matchString": ["mod other"], "caseMode": "sensitive"}"#,
    ));
    assert!(next.get("ignoreCase").is_some(), "{next}");
    let next = read(json_fixture(r#"{"matchString": ["absent"]}"#));
    assert!(next.get("findText").is_some(), "{next}");
}

#[test]
fn literal_bars_preserve_the_exact_match_in_related_searches() {
    let root = Fixture::new();
    let file = root.0.join("mod.txt");
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
        let query = octocode_runtime_base::test_support::opened(
            octocode_runtime_base::id::ToolId::LocalSearch,
            query,
        );
        assert_eq!(query["regexEngine"], "literal", "{calls}");
    }
}

/// A miss that already matched every pattern ignoring case does not run
/// the same patterns again to count case-insensitive hits.
#[test]
fn an_insensitive_miss_runs_its_patterns_once() {
    use octocode_tools_shared::source_view::extraction::RUST_REGEX_RUNS;
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
            false,
        );
        assert_eq!(r.status, ReadStatus::Empty, "{r:?}");
        RUST_REGEX_RUNS.with(std::cell::Cell::get)
    };
    assert_eq!(read("insensitive"), 1);
    assert_eq!(read("sensitive"), 2);
}

/// The warnings of `result` that name a list's entries that matched nothing.
fn unmatched_named(warnings: &[String]) -> Vec<&String> {
    warnings
        .iter()
        .filter(|warning| warning.contains("matchString entries matched nothing"))
        .collect()
}

/// A list `matchString` read with hits names, once, each entry that selected
/// none of them, as written, and no entry that did: within `ranges` too, and
/// under each entry's own case rule.
#[test]
fn a_list_read_names_each_entry_that_matched_nothing() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "alpha\nbeta\nfiller\ngamma\n").expect("fixture");
    for (fields, unmatched) in [
        (
            json_fixture(r#"{"matchString": ["alpha", "nowhere", "beta", "say \"hi\""]}"#),
            vec!["nowhere", "say \"hi\""],
        ),
        (
            json_fixture(r#"{"matchString": ["alpha", "beta"]}"#),
            vec![],
        ),
        (
            json_fixture(
                r#"{
                    "matchString": ["alpha", "gamma"],
                    "ranges": [{"startLine": 1, "endLine": 2}]
                }"#,
            ),
            vec!["gamma"],
        ),
        (
            json_fixture(r#"{"matchString": ["ALPHA", "b.ta"], "regexEngine": "rust"}"#),
            vec!["ALPHA"],
        ),
    ] {
        let result = fetch(&qj(&p, fields.clone()), &Paths(t.0.clone()));
        assert!(
            result.selected_match_count > Some(0),
            "{fields}: {result:?}"
        );
        let named = unmatched_named(&result.warnings);
        if unmatched.is_empty() {
            assert!(named.is_empty(), "{fields}: {named:?}");
            continue;
        }
        assert_eq!(named.len(), 1, "{fields}: {:?}", result.warnings);
        for term in fields["matchString"].as_array().expect("terms") {
            let spelled = serde_json::to_string(term).expect("term");
            assert_eq!(
                named[0].contains(&spelled),
                unmatched.contains(&term.as_str().expect("term")),
                "{fields}: {spelled} in {}",
                named[0]
            );
        }
    }
}

/// A miss reads the file itself again before any folder is searched. A
/// declaration head the file declares otherwise (`pub fn apply(` where it
/// says `pub(crate) fn apply`) reads its one declaration at once, as the
/// read of that line, with a warning naming the swap. With two declarations
/// of the name it is read by its name alone, a fact the hint states; any
/// other miss in a file with an outline leads to that outline. Each miss has
/// one hint and one call: the folder search is the call only of a file with
/// no read of its own to offer.
#[test]
fn a_miss_reads_the_file_again_before_searching_its_folder() {
    let t = Fixture::new();
    let one = t.0.join("hoist.rs");
    let two = t.0.join("twice.rs");
    fs::write(
        &one,
        "pub(crate) fn apply(x: u32) -> u32 {\n    x\n}\npub fn apply_on() {\n    apply(1);\n}\n",
    )
    .expect("fixture");
    fs::write(
        &two,
        "impl A {\n    fn apply(&self) {}\n}\nimpl B {\n    fn apply(&self) {}\n}\n",
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let read = |path: &std::path::Path, pattern: &str| {
        fetch(
            &qj(
                path,
                serde_json::json!({"matchString": [pattern], "block": true}),
            ),
            &paths,
        )
    };
    for head in ["pub fn apply(", "fn apply<T>"] {
        let row = read(&one, head);
        assert_eq!(row.status, ReadStatus::Success, "{head}: {row:?}");
        let content = row.content.as_deref().unwrap_or_default();
        // The declaration's block, and no other line naming it.
        assert!(
            content.contains("pub(crate) fn apply(x: u32)"),
            "{head}: {content}"
        );
        assert!(content.contains("    x"), "{head}: {content}");
        assert!(!content.contains("apply(1)"), "{head}: {content}");
        assert!(
            row.warnings
                .first()
                .is_some_and(|warning| warning.contains(&format!("`{head}`"))
                    && warning.contains("`apply`")
                    && warning.contains("line 1")),
            "{head}: {:?}",
            row.warnings
        );
    }
    let leads = |path: &std::path::Path, pattern: &str| {
        let row = read(path, pattern);
        assert_eq!(row.status, ReadStatus::Empty, "{row:?}");
        let next = serde_json::to_value(row.next.as_ref().expect("next")).expect("json");
        let order = next
            .as_object()
            .expect("calls")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert!(row.warnings.is_empty(), "{row:?}");
        (row.hints, next, order)
    };
    let (hints, next, order) = leads(&two, "pub fn apply(");
    let lead = &next["read"]["input"]["queries"][0];
    assert_eq!(next["read"]["tool"], "localFetch", "{next}");
    assert_eq!(lead["matchString"], serde_json::json!(["apply"]), "{next}");
    assert_eq!(lead["wholeWord"], true, "{next}");
    assert_eq!(lead["block"], true, "{next}");
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("2 lines match `apply`"), "{hints:?}");
    assert_eq!(order, ["read"], "{next}");

    let (hints, next, order) = leads(&one, "nowhere_here");
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert_eq!(
        next["read"]["input"]["queries"][0]["view"], "symbols",
        "{next}"
    );
    assert_eq!(order, ["read"], "{next}");
}

/// The one declaration is read only when its line's text selects it alone:
/// the same text on another line keeps the empty row and its lead.
#[test]
fn a_declaration_whose_line_repeats_keeps_the_lead() {
    let t = Fixture::new();
    let p = t.0.join("again.ts");
    fs::write(
        &p,
        "export function parseEnv(text: string) {\n  return text;\n}\n// export function parseEnv(text: string) {\n",
    )
    .expect("fixture");
    let row = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["const parseEnv"], "block": true}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(row.status, ReadStatus::Empty, "{row:?}");
}

/// A file without a declaration outline has no outline to lead to: the
/// miss leads to the folder search alone.
#[test]
fn a_miss_in_a_file_without_an_outline_searches_its_folder() {
    let t = Fixture::new();
    let p = t.0.join("notes.txt");
    fs::write(&p, "plain notes\n").expect("fixture");
    let row = fetch(
        &qj(&p, serde_json::json!({"matchString": ["nowhere_here"]})),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(row.next.as_ref().expect("next")).expect("json");
    assert!(next.get("read").is_none(), "{next}");
    assert!(next.get("findText").is_some(), "{next}");
}
