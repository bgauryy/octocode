//! Clipped values widen through reads that return them whole.

use super::*;

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
