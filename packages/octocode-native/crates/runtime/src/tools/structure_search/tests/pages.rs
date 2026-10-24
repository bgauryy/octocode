use super::*;

#[test]
fn response_budgets_preserve_every_path_and_source_metadata_once() {
    let root = Fixture::new();
    for dir in ["a", "b/deep", "c"] {
        for n in 0..7 {
            root.write(&format!("{dir}/f{n}.rs"), "x\n".repeat(n + 1));
        }
    }
    for operation in ["tree", "files"] {
        let row = json!({"operation":operation,"path":root.0,"maxDepth":4,"entryType":"file"});
        let whole = run(&root.0, row.clone()).expect("whole");
        let mut expected = listed(&whole);
        expected.sort_by_key(|entry| entry["path"].as_str().expect("path").to_owned());
        assert_eq!(expected.len(), 21);
        for budget in [1, 250, 700] {
            let pages = walk_pages_budget(&root.0, row.clone(), Some(budget));
            assert!(pages.len() > 1);
            let mut actual: Vec<Value> = pages.iter().flat_map(listed).collect();
            actual.sort_by_key(|entry| entry["path"].as_str().expect("path").to_owned());
            assert_eq!(actual, expected, "{operation}, budget {budget}");
            for page in &pages {
                assert!(page["pagination"].get("pageSize").is_none(), "{page}");
                assert!(
                    page.get("calls").is_none(),
                    "private call bundle leaked: {page}"
                );
            }
        }
    }
}

#[test]
fn a_private_page_past_end_is_explicitly_out_of_range() {
    let root = Fixture::new();
    root.write("a.rs", "a\n");
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"page":999}),
    )
    .expect("private page");
    assert_eq!(out["pagination"]["outOfRange"], true, "{out}");
    assert!(listed(&out).is_empty(), "{out}");
}

#[test]
fn automatic_page_costs_count_each_opened_directory_header() {
    use super::super::cut::{RowCost, page_ranges};
    let row = |continues| RowCost {
        entry: 10,
        header: 25,
        continues,
    };
    assert_eq!(
        page_ranges(&[false, true, true, true, true, true].map(row), 60),
        [0..3, 3..6]
    );
    assert_eq!(
        page_ranges(&[false, false, true].map(row), 60),
        [0..1, 1..3]
    );
    assert_eq!(page_ranges(&[false, true].map(row), 1), [0..1, 1..2]);
}
