//! The pull-request menu, the merge-commit read and both diff sides.
use super::*;

/// A file GitHub sent without a readable patch (tooLarge, omitted)
/// is read at both sides, whole, ahead of the most-changed patched
/// file; a binary file or a GraphQL node (no blob sha) never is.
#[test]
fn change_reads_prefer_the_first_unpatched_text_file() {
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
        .map(|(name, read)| (*name, read["query"]["queries"][0].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            (
                "readAtCommit",
                json!({"owner":"o","repo":"r","path":"src/huge.rs","ref":"head"})
            ),
            (
                "readParent",
                json!({"owner":"o","repo":"r","path":"src/huge.rs","ref":"base"})
            ),
        ]
    );
    // Without an unpatched text file the top patched file is read.
    let reads = change_reads("o", "r", &files[..2], &sides, |_| true);
    assert_eq!(reads[0].1["query"]["queries"][0]["path"], "src/big.rs");
    let mut graphql = files[2].clone();
    graphql.as_object_mut().map(|file| file.remove("sha"));
    assert!(first_unpatched(&[graphql], |_| true).is_none());
}
