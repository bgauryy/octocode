//! The pull-request menu, the merge-commit read and both diff sides.
use super::*;
use octocode_test_support::json_fixture;

/// A file GitHub sent without a readable patch (tooLarge, omitted)
/// is read at both sides, whole; a delivered patch, a binary file or a
/// GraphQL node (no blob sha) never is.
#[test]
fn change_reads_prefer_unpatched_text_files() {
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: Some("base"),
    };
    let file = |name: &str, additions: u64, patch: Option<&str>| {
        let mut file = json!({"filename":name,"status":"modified","additions":additions,
            "deletions":0,"sha":"1111111111111111111111111111111111111111"});
        if let Some(patch) = patch {
            file["patch"] = json!(patch);
        }
        file
    };
    let files = [
        file("src/big.rs", 900, Some("@@ -1,1 +1,2 @@\n x\n+y")),
        file("assets/logo.png", 0, None),
        file("src/huge.rs", 70_000, None),
    ];
    let reads = change_reads("o", "r", &files, &sides, |_| true);
    let paths = reads
        .iter()
        .map(|(name, read)| (name.as_str(), read["input"]["queries"][0].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            (
                "readAtCommit",
                json_fixture(
                    r#"{
                        "owner": "o",
                        "repo": "r",
                        "path": "src/huge.rs",
                        "ref": "head"
                    }"#
                )
            ),
            (
                "readParent",
                json_fixture(
                    r#"{
                        "owner": "o",
                        "repo": "r",
                        "path": "src/huge.rs",
                        "ref": "base"
                    }"#
                )
            ),
        ]
    );
    for (_, row) in &paths {
        assert_eq!(
            octocode_tools_shared::test_support::read_view(
                octocode_runtime_base::id::ToolId::GhGetFileContent,
                row
            ),
            octocode_contract_types::ReadView::Content
        );
    }
    // Without an unpatched text file nothing is read: a delivered patch is
    // the evidence itself.
    assert!(change_reads("o", "r", &files[..2], &sides, |_| true).is_empty());
    let mut graphql = files[2].clone();
    graphql.as_object_mut().map(|file| file.remove("sha"));
    assert!(unpatched(&[graphql], |_| true).is_empty());
}

/// Every file GitHub withheld the patch of gets its own required read at
/// each side, however many there are: the calls stay within one call's
/// queries limit and together name each file once per side.
#[test]
fn every_unpatched_file_is_read_at_each_side() {
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: Some("base"),
    };
    let per_call = octocode_runtime_base::response::result::rows_per_call(ToolId::GhGetFileContent);
    let count = per_call * 2 + 1;
    let files = (0..count)
        .map(|n| {
            json!({"filename":format!("src/huge{n}.rs"),"status":"modified","additions":9000,
            "deletions":0,"sha":"1111111111111111111111111111111111111111"})
        })
        .collect::<Vec<_>>();
    let reads = change_reads("o", "r", &files, &sides, |_| true);
    let mut seen = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for (name, call) in &reads {
        assert!(names.insert(name.clone()), "duplicate lead name {name}");
        assert_eq!(call["kind"], "continue", "{name}");
        let rows = call["input"]["queries"].as_array().expect("queries");
        assert!(rows.len() <= per_call, "{name} holds {} rows", rows.len());
        for row in rows {
            let key = (row["path"].clone(), row["ref"].clone());
            assert!(seen.insert(key), "read twice: {row}");
        }
    }
    assert_eq!(seen.len(), count * 2, "{reads:?}");
}
