use super::*;
use crate::tools::gh_get_history_item::file_page::read_commit_lead;

fn request(sections: &[&str]) -> HistoryItemRequest {
    HistoryItemRequest::from_row(json!({
        "operation":"commit", "owner":"o", "repo":"r", "ref":"head",
        "sections":sections
    }))
    .expect("commit request")
}

#[test]
fn section_expansion_is_optional_but_unread_pages_are_required() {
    let query = request(&[]);
    let mut output = json!({});
    attach_diff_continuations(
        &mut output,
        &query,
        ItemOperation::Commit,
        Some("head"),
        DiffCursors {
            files_listed: true,
            file_page: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(
        output["calls"]["nextFilePage"]["kind"], "continue",
        "{output}"
    );
    assert_eq!(
        output["calls"]["readPatches"]["kind"], "related",
        "{output}"
    );
    let summary = read_commit_lead(&request(&["patches"]), "head");
    assert_eq!(summary["kind"], "related", "{summary}");
    assert_eq!(summary["input"]["queries"][0]["ref"], "head");
}

#[test]
fn pull_request_menus_expand_only_unrequested_sections() {
    for sections in [vec![], vec!["files"], vec!["patches"], vec!["body"]] {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest", "owner":"o", "repo":"r", "number":1,
            "sections":sections
        }))
        .expect("pull request");
        let menu = pr_next_menu(
            &query,
            &["src/a.rs".into()],
            &json!({
                "body":"description", "changed_files":30, "additions":900,
                "deletions":10, "comments":3
            }),
        );
        let calls = menu.as_object().expect("menu");
        assert!(!calls.is_empty(), "{menu}");
        for call in calls.values() {
            assert_eq!(call["kind"], "related", "{menu}");
        }
    }
}

#[test]
fn source_recovery_is_required_only_when_requested_patch_is_missing() {
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: Some("parent"),
    };
    for (patch, expected) in [
        (Some("@@ -1 +1 @@\n-old\n+new"), "related"),
        (None, "continue"),
    ] {
        let mut file = json!({
            "filename":"src/a.rs", "sha":"blob", "status":"modified",
            "additions":1, "deletions":1
        });
        if let Some(patch) = patch {
            file["patch"] = json!(patch);
        }
        let reads = change_reads("o", "r", &[file], &sides, |_| true);
        assert_eq!(reads.len(), 2);
        for (_, read) in reads {
            assert_eq!(read["kind"], expected, "{read}");
        }
    }
}
