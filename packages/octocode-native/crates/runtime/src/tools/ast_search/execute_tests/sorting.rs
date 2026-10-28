use super::*;

#[test]
fn match_files_obey_both_sort_directions_in_every_view() {
    let root = tempfile::tempdir().unwrap();
    for (name, count) in [("a.rs", 2), ("b.rs", 1), ("c.rs", 2), ("d.rs", 3)] {
        let body = "hit();".repeat(count);
        std::fs::write(root.path().join(name), format!("fn run() {{ {body} }}")).unwrap();
    }
    let (paths, security) = crate::tools::test_support::simple_policy(root.path());
    for view in ["content", "files", "countMatches"] {
        for (by, order, expected) in [
            ("matchCount", "asc", ["b.rs", "a.rs", "c.rs", "d.rs"]),
            ("matchCount", "desc", ["d.rs", "a.rs", "c.rs", "b.rs"]),
            ("path", "asc", ["a.rs", "b.rs", "c.rs", "d.rs"]),
            ("path", "desc", ["d.rs", "c.rs", "b.rs", "a.rs"]),
        ] {
            let out = execute_row(
                json!({"operation":"match","path":root.path(),"parser":"rust",
                    "rule":{"pattern":"hit()"},"view":view,"sort":{"by":by,"order":order}}),
                &paths,
                &security,
                &crate::execution::cancel::NeverCancel,
            )
            .unwrap();
            let names = out["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|file| {
                    std::path::Path::new(file["path"].as_str().unwrap())
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(names, expected, "{view} {by} {order}: {out}");
        }
    }
}
