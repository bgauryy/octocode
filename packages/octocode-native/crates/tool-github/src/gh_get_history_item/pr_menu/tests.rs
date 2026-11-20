//! The pull-request menu, the merge-commit read and both diff sides.
use super::*;
use octocode_test_support::json_fixture;

/// A file GitHub sent without a readable patch (tooLarge, omitted)
/// is read at both sides, whole, ahead of the most-changed patched
/// file; a binary file or a GraphQL node (no blob sha) never is.
#[test]
fn change_reads_prefer_unpatched_text_files() {
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: Some("base"),
        needle: None,
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
    // Without an unpatched text file the top patched file is read.
    let reads = change_reads("o", "r", &files[..2], &sides, |_| true);
    assert_eq!(reads[0].1["input"]["queries"][0]["path"], "src/big.rs");
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
        needle: None,
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

/// A `matchString` read offers the source of every file whose hunks hold it,
/// both sides each, and none of a file with no hit. The rows ride related
/// calls within one call's queries limit, each under its own name.
#[test]
fn change_reads_cover_every_file_a_match_string_hits() {
    let needle = crate::gh_get_history_item::matcher::Matcher::of(
        &serde_json::from_value::<GhGetHistoryItemQuery>(json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "o",
                "repo": "r",
                "number": 1,
                "matchString": ["needle"]
            }"#,
        ))
        .expect("query"),
    )
    .expect("matcher")
    .expect("a literal");
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: Some("base"),
        needle: Some(&needle),
    };
    let file = |name: &str, line: &str| {
        json!({"filename":name,"status":"modified","additions":1,"deletions":0,
            "sha":"1111111111111111111111111111111111111111",
            "patch":format!("@@ -1,2 +1,3 @@\n a\n+{line}\n b")})
    };
    let files = [
        file("src/a.rs", "needle one"),
        file("src/b.rs", "needle two"),
        file("src/miss.rs", "other"),
        file("src/c.rs", "needle three"),
        file("src/d.rs", "needle four"),
    ];
    let reads = change_reads("o", "r", &files, &sides, |_| true);
    let per_call = octocode_runtime_base::response::result::rows_per_call(ToolId::GhGetFileContent);
    let mut names = std::collections::HashSet::new();
    let mut read = Vec::new();
    for (name, call) in &reads {
        assert!(names.insert(name.clone()), "duplicate lead name {name}");
        assert_eq!(call["kind"], "related", "{name}");
        let rows = call["input"]["queries"].as_array().expect("queries");
        assert!(rows.len() <= per_call, "{name} holds {} rows", rows.len());
        read.extend(rows.iter().map(|row| {
            (
                row["path"].as_str().unwrap_or("").to_owned(),
                row["ref"].as_str().unwrap_or("").to_owned(),
            )
        }));
    }
    read.sort();
    let expected = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"]
        .iter()
        .flat_map(|path| {
            [
                (path.to_string(), "base".to_owned()),
                (path.to_string(), "head".to_owned()),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(read, expected);
}
