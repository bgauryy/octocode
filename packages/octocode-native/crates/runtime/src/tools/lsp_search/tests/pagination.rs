use super::*;
use serde_json::json;

#[test]
fn semantic_snapshots_ignore_only_result_page_progress() {
    let first = query(json!({"operation":"references","path":"a.ts","symbolName":"run","line":4}));
    let later = requery(&first, json!({"page":2,"snapshot":"prior"}));
    let items = vec![json!({"path":"a.ts","line":4})];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&later, "references", &items)
    );
    for changed in [
        json!({"includeDeclaration":false}),
        json!({"occurrence":1}),
        json!({"contextLines":{"before":1,"after":3}}),
    ] {
        assert_ne!(
            semantic_snapshot(&later, "references", &items),
            semantic_snapshot(&requery(&later, changed), "references", &items)
        );
    }
}

#[test]
fn stale_and_unpinned_pages_cannot_silently_reacquire() {
    let first = query(json!({"operation":"references","path":"a.ts","symbolName":"run","line":1}));
    let bare = requery(&first, json!({"page":2}));
    assert!(super::super::unpinned_later_page(&bare));
    let pinned = requery(&bare, json!({"snapshot":"old"}));
    assert!(snapshot_mismatch(&pinned, "new"));
    for row in [
        snapshot_changed(&pinned),
        super::super::unpinned_page_row(&bare),
    ] {
        assert!(row.get("next").is_none());
        let restart = &row["related"][0];
        assert_eq!(restart["kind"], "related");
        let input = &restart["input"]["queries"][0];
        for field in crate::contracts::state_fields(ToolId::LspSearch) {
            assert!(input.get(field).is_none(), "{field}: {input}");
        }
        assert!(crate::contracts::validate("lspSearch", restart["input"].clone()).is_ok());
    }
}

#[test]
fn terminal_cursor_limit_preserves_evidence_and_declares_the_gap() {
    let q = query(json!({"operation":"references","path":"a.ts","symbolName":"run","line":1}));
    let mut row =
        json!({"matches":[{"path":"a.ts","line":1}],"pagination":{"page":1000,"hasMore":true}});
    offer_next_page(
        &q,
        &mut row,
        Some(NextPage {
            page: 1001,
            snapshot: "lsp-v3:captured".into(),
        }),
    );
    assert!(row.get("next").is_none());
    assert_eq!(row["matches"].as_array().expect("evidence").len(), 1);
    assert_eq!(row["terminalLimit"], true);
    assert!(
        row["partialReasons"]
            .as_array()
            .expect("disclosure")
            .contains(&json!(crate::response::paging::CURSOR_MAXIMUM))
    );
}
