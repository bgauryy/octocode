use super::*;
use crate::gh_get_history_item::file_page::read_commit_lead;
use octocode_test_support::json_fixture;

fn request(sections: &[&str]) -> HistoryItemRequest {
    HistoryItemRequest::from_row(json!({
        "operation":"commit", "owner":"o", "repo":"r", "ref":"head",
        "sections":sections
    }))
    .expect("commit request")
}

#[test]
fn unread_pages_are_required_and_no_static_section_view_is_offered() {
    let query = request(&[]);
    let mut output = json!({});
    attach_diff_continuations(
        &mut output,
        &query,
        ItemOperation::Commit,
        Some("head"),
        DiffCursors {
            file_page: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(output["next"]["files"]["kind"], "continue", "{output}");
    assert!(output["calls"].get("readPatches").is_none(), "{output}");
    let summary = read_commit_lead(&request(&["patches"]), "head").expect("lead");
    assert_eq!(summary["kind"], "related", "{summary}");
    assert_eq!(summary["input"]["queries"][0]["ref"], "head");
}

#[test]
fn source_recovery_is_required_only_when_requested_patch_is_missing() {
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: Some("parent"),
        needle: None,
    };
    // Missing source is two continuations, one per side; delivered
    // source is optional, one related call reading both sides.
    for (patch, expected, calls) in [
        (Some("@@ -1 +1 @@\n-old\n+new"), "related", 1),
        (None, "continue", 2),
    ] {
        let mut file = json_fixture(
            r#"{
                "filename": "src/a.rs",
                "sha": "blob",
                "status": "modified",
                "additions": 1,
                "deletions": 1
            }"#,
        );
        if let Some(patch) = patch {
            file["patch"] = json!(patch);
        }
        let reads = change_reads("o", "r", &[file], &sides, |_| true);
        assert_eq!(reads.len(), calls);
        let rows = reads
            .iter()
            .map(|(_, read)| read["input"]["queries"].as_array().map_or(0, Vec::len))
            .sum::<usize>();
        assert_eq!(rows, 2, "both sides: {reads:?}");
        for (_, read) in reads {
            assert_eq!(read["kind"], expected, "{read}");
        }
    }
}
