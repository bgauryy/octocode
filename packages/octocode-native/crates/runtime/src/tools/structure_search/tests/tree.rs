use super::*;

#[test]
fn tree_depth_is_explicit_and_filters_do_not_implicitly_widen_it() {
    let root = Fixture::new();
    root.write("top.rs", "x");
    root.write("a/mid.rs", "xx");
    root.write("a/b/leaf.rs", "xxx");
    for (depth, expected) in [(1, 1), (2, 2), (3, 3)] {
        let out = run(
            &root.0,
            json!({"operation":"tree","path":root.0,"maxDepth":depth,"extensions":["rs"]}),
        )
        .expect("tree");
        assert_eq!(listed(&out).len(), expected, "{out}");
    }
    let default = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"extensions":["rs"]}),
    )
    .expect("default depth");
    assert_eq!(listed(&default).len(), 1, "{default}");
    let directories = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"maxDepth":3,"entryType":"directory"}),
    )
    .expect("dirs");
    assert!(
        listed(&directories)
            .iter()
            .all(|entry| entry["type"] == "directory"),
        "{directories}"
    );
}

#[test]
fn sort_keys_order_evidence_in_both_directions() {
    let root = Fixture::new();
    for (name, count) in [("a.rs", 3), ("b.rs", 1), ("c.rs", 2)] {
        root.write(name, "x\n".repeat(count));
    }
    for by in ["name", "path", "sizeBytes", "lineCount"] {
        let mut outputs = Vec::new();
        for order in ["asc", "desc"] {
            let query = json!({"operation":"files","path":root.0,"entryType":"file","sort":{"by":by,"order":order}});
            let out = run(&root.0, query.clone()).expect("sorted");
            crate::tools::test_support::assert_row_contract(
                crate::id::ToolId::StructureSearch,
                &out,
            );
            let entries = listed(&out);
            if ["sizeBytes", "lineCount"].contains(&by) {
                let key = if by == "sizeBytes" {
                    "size"
                } else {
                    "totalLines"
                };
                let values: Vec<u64> = entries
                    .iter()
                    .map(|row| row[key].as_u64().expect("sort value"))
                    .collect();
                assert!(
                    values.windows(2).all(|pair| if order == "asc" {
                        pair[0] <= pair[1]
                    } else {
                        pair[0] >= pair[1]
                    }),
                    "{out}"
                );
            }
            outputs.push(paths(&out));
        }
        outputs[1].reverse();
        assert_eq!(outputs[0], outputs[1], "{by}");
    }
}
