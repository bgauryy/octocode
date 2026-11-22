use super::*;
use octocode_test_support::json_fixture;

#[test]
fn unresolved_anchors_preserve_requested_spelling_and_offer_explicit_related_work() {
    let q = query(json_fixture(
        r#"{
            "operation": "references",
            "path": "/repo/a.ts",
            "symbolName": "renderX",
            "line": 100,
            "occurrence": 99
        }"#,
    ));
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::NotFound,
        "no exact occurrence",
        true,
    );
    anchor_recovery(&mut row, &q, Some("function render() {}\nrender();"));
    assert!(row.get("next").is_none());
    // The file never spells `renderX` (in any case): a read for it would
    // find nothing, so the recovery is searches: the workspace's, the file's
    // text and its declarations.
    let related = row["related"].as_array().expect("search recovery");
    assert_eq!(related.len(), 3, "{row}");
    assert!(related.iter().all(|call| call["tool"] != "localFetch"));
    for call in related {
        assert_eq!(call["kind"], "related");
        octocode_runtime_base::contracts::validate(
            call["tool"].as_str().expect("tool"),
            call["input"].clone(),
        )
        .expect("runnable related input");
    }
    let search = &related
        .iter()
        .find(|call| call["tool"] == "lspSearch")
        .expect("search")["input"]["queries"][0];
    assert_eq!(search["symbolFuzzy"], "renderX");
    assert!(search.get("occurrence").is_none());
    assert!(search.get("line").is_none());
}

/// An anchor that resolves to nothing leads to the two ways of finding the
/// line, each as a call that runs as given: a localSearch of the file for the
/// spelling and an astSearch of its declarations for the name, whether the
/// line is past the end of the file or the name is not spelled near it.
#[test]
fn unresolved_anchors_lead_to_localsearch_and_astsearch_for_the_line() {
    for (name, line, source) in [
        ("foo", 99, "function foo() {}\n\n\nfoo();\nfoo();\n"),
        ("nothere", 1, "function foo() {}\n"),
    ] {
        let q = query(json_fixture(&format!(
            r#"{{"operation": "callers", "path": "/repo/a.ts", "symbolName": "{name}", "line": {line}}}"#
        )));
        let mut row = failure(
            &q,
            "file:///repo/a.ts",
            ErrorCode::NotFound,
            "missing",
            true,
        );
        anchor_recovery(&mut row, &q, Some(source));
        let related = row["related"].as_array().expect("related");
        let lead = |tool: &str| {
            related
                .iter()
                .find(|call| call["tool"] == tool)
                .unwrap_or_else(|| panic!("a {tool} lead: {row}"))
        };
        let search = &lead("localSearch")["input"]["queries"][0];
        assert_eq!(search["path"], "/repo/a.ts", "{row}");
        assert!(
            search["matchString"][0]
                .as_str()
                .is_some_and(|pattern| pattern.contains(name)),
            "{row}"
        );
        let symbols = &lead("astSearch")["input"]["queries"][0];
        assert_eq!(symbols["operation"], "symbols", "{row}");
        assert_eq!(symbols["path"], "/repo/a.ts", "{row}");
        assert_eq!(symbols["symbolFilter"]["terms"], json!([name]), "{row}");
        for call in related {
            octocode_runtime_base::contracts::validate(
                call["tool"].as_str().expect("tool"),
                call["input"].clone(),
            )
            .unwrap_or_else(|error| panic!("{call}: {error:?}"));
        }
        let hint = row["hints"].to_string();
        assert!(
            hint.contains("localSearch") && hint.contains("astSearch"),
            "{row}"
        );
    }
}
