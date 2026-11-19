//! F2 groups: one grouping and one pricer for every path-only view.
use super::*;

fn entry(dir: &str, item: Value) -> Entry {
    Entry {
        dir: dir.to_owned(),
        item,
    }
}

/// Walk order merges a directory the listing returns to into its group; a
/// rank opens a new group for it, so the rank order stays the answer.
#[test]
fn merged_groups_join_a_directory_and_runs_keep_the_rank() {
    let entries = || {
        vec![
            entry("src", json!("a.rs")),
            entry("src/x", json!("b.rs")),
            entry("src", json!("c.rs")),
        ]
    };
    assert_eq!(
        groups(entries(), Order::Merged),
        json!([{"dir": "src", "files": ["a.rs", "c.rs"]}, {"dir": "src/x", "files": ["b.rs"]}])
            .as_array()
            .expect("groups")
            .clone()
    );
    assert_eq!(
        groups(entries(), Order::Runs),
        json!([
            {"dir": "src", "files": ["a.rs"]},
            {"dir": "src/x", "files": ["b.rs"]},
            {"dir": "src", "files": ["c.rs"]},
        ])
        .as_array()
        .expect("groups")
        .clone()
    );
}

/// An entry costs its item and a comma, plus its group's header when it
/// opens the group; the pricer and the groups agree on what a page holds.
#[test]
fn each_entry_costs_its_item_and_the_header_it_opens() {
    let costs = costs([("src", 6), ("src", 6), ("lib", 6)]);
    let header = header_chars("src");
    assert_eq!(
        costs[0],
        Cost {
            entry: 7,
            header,
            continues: false
        }
    );
    assert_eq!(
        costs[1],
        Cost {
            entry: 7,
            header,
            continues: true
        }
    );
    assert_eq!(costs[2].header, header_chars("lib"));
    assert!(!costs[2].continues);
    // The whole listing, priced, is the groups' length.
    let grouped = groups(
        [
            entry("src", json!("a.rs")),
            entry("src", json!("b.rs")),
            entry("lib", json!("c.rs")),
        ],
        Order::Runs,
    );
    let item = octocode_engine::json_size::utf16_units(&json!("a.rs")).expect("size");
    let priced: usize = super::costs([("src", item), ("src", item), ("lib", item)])
        .iter()
        .map(|cost| cost.entry + if cost.continues { 0 } else { cost.header })
        .sum();
    let shown = octocode_engine::json_size::utf16_units(&grouped).expect("size");
    // The pricer counts a comma after every item and header: one more per
    // group than the groups render inside the list's brackets, so it never
    // prices a page shorter than it renders.
    assert_eq!(priced, shown - 2 + grouped.len() + 1, "{grouped:?}");
}

/// Hoist L-5: rows grouped only where the groups are shorter, so no view
/// grows; every row reads back by `dir + "/" + name` with its own fields.
#[test]
fn rows_are_grouped_only_where_that_is_shorter_and_read_back_whole() {
    let deep = (0..6)
        .map(|n| json!({"path": format!("packages/octocode-native/crates/tool-local/src/f{n}.rs")}))
        .collect::<Vec<_>>();
    let grouped = group_rows(&deep, Order::Merged).expect("shorter grouped");
    assert_eq!(grouped.len(), 1);
    assert_eq!(rows_of(&grouped), deep);
    let counted = (0..4)
        .map(|n| json!({"path": format!("src/deep/dir/f{n}.rs"), "matchCount": n + 1}))
        .collect::<Vec<_>>();
    let grouped = group_rows(&counted, Order::Merged).expect("shorter grouped");
    assert_eq!(
        grouped[0]["files"][1],
        json!({"name": "f1.rs", "matchCount": 2})
    );
    assert_eq!(rows_of(&grouped), counted);
    // Flat paths in distinct directories, or one row, stay flat.
    let flat = [
        json!({"path": "a/x.rs"}),
        json!({"path": "b/y.rs"}),
        json!({"path": "z.rs"}),
    ];
    assert_eq!(group_rows(&flat, Order::Merged), None);
    assert_eq!(group_rows(&deep[..1], Order::Merged), None);
    assert_eq!(
        group_rows(&[json!("not a row"), json!("x")], Order::Merged),
        None
    );
}

/// A root entry names itself; an absolute path keeps its leading `/`.
#[test]
fn a_path_splits_into_its_directory_and_name_and_back() {
    for path in [
        "README.md",
        "src/lib.rs",
        "/abs/file.rs",
        "/top.rs",
        "a/b/c.d.ts",
    ] {
        let (dir, name) = split(path);
        assert_eq!(path_of(dir, name), path, "{path}");
    }
    assert_eq!(split("README.md"), (".", "README.md"));
}
