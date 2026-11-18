use super::*;
use octocode_test_support::json_fixture;

#[test]
fn semantic_snapshots_ignore_only_result_page_progress() {
    let first = query(json_fixture(
        r#"{
            "operation": "references",
            "path": "a.ts",
            "symbolName": "run",
            "line": 4
        }"#,
    ));
    let later = requery(&first, json_fixture(r#"{"page": 2, "snapshot": "prior"}"#));
    let items = vec![json_fixture(r#"{"path": "a.ts", "line": 4}"#)];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&later, "references", &items)
    );
    for changed in [
        json_fixture(r#"{"includeDeclaration": false}"#),
        json_fixture(r#"{"occurrence": 1}"#),
        json_fixture(r#"{"contextLines": {"before": 1, "after": 3}}"#),
    ] {
        assert_ne!(
            semantic_snapshot(&later, "references", &items),
            semantic_snapshot(&requery(&later, changed), "references", &items)
        );
    }
}

#[test]
fn stale_and_unpinned_pages_cannot_silently_reacquire() {
    let first = query(json_fixture(
        r#"{
            "operation": "references",
            "path": "a.ts",
            "symbolName": "run",
            "line": 1
        }"#,
    ));
    let bare = requery(&first, json_fixture(r#"{"page": 2}"#));
    assert!(super::super::unpinned_later_page(&bare));
    let pinned = requery(&bare, json_fixture(r#"{"snapshot": "old"}"#));
    assert!(snapshot_mismatch(&pinned, "new"));
    for row in [
        snapshot_changed(&pinned),
        super::super::unpinned_page_row(&bare),
    ] {
        assert!(row.get("next").is_none());
        let restart = &row["related"][0];
        assert_eq!(restart["kind"], "related");
        let input = &restart["input"]["queries"][0];
        for field in octocode_runtime_base::contracts::state_fields(ToolId::LspSearch) {
            assert!(input.get(field).is_none(), "{field}: {input}");
        }
        assert!(
            octocode_runtime_base::contracts::validate("lspSearch", restart["input"].clone())
                .is_ok()
        );
    }
}

/// A page past the former 1,000-page schema ceiling still continues: the
/// cursor has no maximum, so the row keeps its evidence and its next page.
#[test]
fn a_page_past_the_former_ceiling_continues() {
    let q = query(json_fixture(
        r#"{
            "operation": "references",
            "path": "a.ts",
            "symbolName": "run",
            "line": 1
        }"#,
    ));
    let mut row = json_fixture(
        r#"{
            "matches": [{"path": "a.ts", "line": 1}],
            "pagination": {"page": 1000, "hasMore": true}
        }"#,
    );
    offer_next_page(
        &q,
        &mut row,
        Some(NextPage {
            page: 1001,
            snapshot: "lsp-v3:captured".into(),
        }),
    );
    assert_eq!(
        row["next"]["nextPage"]["input"]["queries"][0]["page"], 1001,
        "{row}"
    );
    assert_eq!(row["matches"].as_array().expect("evidence").len(), 1);
    assert!(row.get("terminalLimit").is_none(), "{row}");
}
