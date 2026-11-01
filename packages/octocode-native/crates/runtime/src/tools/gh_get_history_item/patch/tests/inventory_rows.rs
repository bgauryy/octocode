//! File inventory: compact grouped rows, file filters, page sizes and review selection.
use super::*;

fn inventory_request(fields: Value) -> HistoryItemRequest {
    patch_request(crate::tools::gh_get_history_item::util::merge(
        json!({"sections":["files"]}),
        fields,
    ))
}

fn listed(name: &str, status: &str, additions: u64, deletions: u64, patch: bool) -> Value {
    let mut file = json!({"sha":"1","filename":name,"status":status,
        "additions":additions,"deletions":deletions});
    if patch {
        file["patch"] = json!("@@ -1 +1 @@");
    }
    file
}

fn inventory(files: Vec<Value>, query: &HistoryItemRequest) -> (Value, Map<String, Value>) {
    let mut row = json!({});
    let mut pagination = Map::new();
    let scope = FileScope::from_query(query).expect("valid file filter");
    shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        query,
        false,
        scope.as_ref(),
    );
    (row, pagination)
}

/// A patch-free inventory row is one compact string; files of one
/// directory share a `{"dir/": [...]}` group so the prefix is written
/// once. Patchless reasons and rename origins survive.
#[test]
fn inventory_rows_are_compact_and_group_directory_files() {
    let mut renamed = listed("src/new.rs", "renamed", 0, 0, false);
    renamed["previous_filename"] = json!("lib/old.rs");
    let files = vec![
        listed(".eslintrc", "modified", 1, 1, true),
        listed("src/a.ts", "added", 8, 0, true),
        listed("src/checker.ts", "modified", 39_550, 39_342, false),
        renamed,
        listed("src/ui/logo.png", "added", 0, 0, false),
        listed("src/z.ts", "removed", 0, 5, true),
        listed("docs/omitted.md", "modified", 0, 0, false),
    ];
    let (row, pagination) = inventory(files, &inventory_request(json!({})));
    assert_eq!(
        row["files"],
        json!([
            "M +1 -1 .eslintrc",
            {"src/": [
                "A +8 -0 a.ts",
                "M +39550 -39342 !tooLarge checker.ts",
                "R +0 -0 new.rs <- lib/old.rs",
                "D +0 -5 z.ts"
            ]},
            "A +0 -0 !binary src/ui/logo.png",
            "M !omitted docs/omitted.md"
        ])
    );
    assert_eq!(pagination["files"]["totalItems"], 7);
}

/// A directory appears once in a page's inventory: files of one
/// directory that a deeper directory's files separate in provider order
/// still share the one group, which sits where the directory first appears.
#[test]
fn inventory_merges_a_directory_split_by_a_deeper_directory() {
    let files = vec![
        listed("src/a.ts", "modified", 1, 1, true),
        listed("src/sub/deep.ts", "added", 2, 0, true),
        listed("src/b.ts", "modified", 1, 1, true),
        listed("src/sub/other.ts", "modified", 1, 1, true),
        listed("tail.md", "modified", 1, 1, true),
        listed("src/c.ts", "removed", 0, 5, true),
    ];
    let (row, pagination) = inventory(files, &inventory_request(json!({})));
    assert_eq!(
        row["files"],
        json!([
            {"src/": ["M +1 -1 a.ts", "M +1 -1 b.ts", "D +0 -5 c.ts"]},
            {"src/sub/": ["A +2 -0 deep.ts", "M +1 -1 other.ts"]},
            "M +1 -1 tail.md"
        ])
    );
    assert_eq!(pagination["files"]["totalItems"], 6);
}

/// `include`/`status`/`minChanges` narrow the inventory by status, path glob or prefix and
/// change count; totals count matches only. A file GitHub sent without
/// line counts is not "small": it passes `minChanges`.
#[test]
fn file_filter_narrows_by_status_path_and_change_count() {
    let files = || {
        vec![
            listed("src/a.ts", "modified", 3, 1, true),
            listed("src/deep/b.ts", "added", 40, 0, true),
            listed("src/c.rs", "modified", 50, 0, true),
            listed("docs/readme.md", "modified", 9, 9, true),
            listed("src/huge.ts", "modified", 0, 0, false),
        ]
    };
    let names = |fields: Value| {
        let (row, pagination) = inventory(files(), &inventory_request(fields));
        let mut out = Vec::new();
        for item in row["files"].as_array().into_iter().flatten() {
            match item {
                Value::String(row) => out.push(row.rsplit(' ').next().unwrap_or("").to_owned()),
                Value::Object(group) => {
                    for (dir, rows) in group {
                        for row in rows.as_array().into_iter().flatten() {
                            let name = row.as_str().unwrap_or("").rsplit(' ').next().unwrap_or("");
                            out.push(format!("{dir}{name}"));
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(pagination["files"]["totalItems"], out.len());
        out
    };
    // "**" spans zero or more directories; "*" stays in one segment.
    assert_eq!(
        names(json!({"include":["src/**/*.ts"]})),
        ["src/a.ts", "src/huge.ts", "src/deep/b.ts"]
    );
    assert_eq!(
        names(json!({"include":["src/*.ts"]})),
        ["src/a.ts", "src/huge.ts"]
    );
    assert_eq!(
        names(json!({"include":["**/*.ts"]})),
        ["src/a.ts", "src/huge.ts", "src/deep/b.ts"]
    );
    assert_eq!(
        names(json!({"include":["src/deep/**", "docs/**"]})),
        ["src/deep/b.ts", "docs/readme.md"]
    );
    assert_eq!(names(json!({"fileStatus":["added"]})), ["src/deep/b.ts"]);
    assert_eq!(
        names(json!({"include":["**/*.ts"],"fileStatus":["modified"],"minChanges":10})),
        ["src/huge.ts"]
    );
    let invalid = inventory_request(json!({"include":["src/[a"]}));
    assert!(FileScope::from_query(&invalid).is_err());
}

/// An omitted pageSize sizes a patch-free inventory to the response page
/// and a patch read to one provider batch (one continuation stream walks
/// every patch of such a PR); an explicit one may exceed a provider
/// batch only without patches.
#[test]
fn inventory_page_size_fills_the_response_page() {
    let mut query = inventory_request(json!({}));
    query.auto_page_chars = Some(50_000);
    assert_eq!(file_page_size(&query, false), 833);
    assert_eq!(file_page_size(&query, true), MAX_PER_PAGE);
    let small = inventory_request(json!({"pageSize":5}));
    assert_eq!(file_page_size(&small, true), 5);
    query.auto_page_chars = Some(1_000);
    assert_eq!(file_page_size(&query, false), MAX_PER_PAGE);
    let explicit = inventory_request(json!({"pageSize":1000}));
    assert_eq!(file_page_size(&explicit, false), 1_000);
    assert_eq!(file_page_size(&explicit, true), MAX_PER_PAGE);
    let files = (0..700)
        .map(|i| listed(&format!("d{}/f{i}.rs", i % 2), "modified", 1, 1, true))
        .collect::<Vec<_>>();
    let (row, pagination) = inventory(files, &explicit);
    // Two directories, each one group of 350 rows.
    let rows: usize = row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group.as_object().into_iter().flat_map(|g| g.values()))
        .filter_map(|rows| rows.as_array().map(Vec::len))
        .sum();
    assert_eq!(rows, 700);
    assert_eq!(row["files"].as_array().map(Vec::len), Some(2));
    assert_eq!(pagination["files"]["hasMore"], false);
}

/// The review pick names every source file by churn within
/// one budget, skipping tests, docs and lockfiles.
#[test]
fn review_selection_packs_source_files_by_churn() {
    let f = |name: &str, changed: u64| json!({"filename":name,"additions":changed,"deletions":0});
    let files = vec![
        f(".changeset/x.md", 8),
        f("pydantic/json_schema.py", 40),
        f("tests/test_counter.py", 400),
        f("pydantic/_known_annotated_metadata.py", 12),
        f("pydantic/fields.py", 90),
        f("uv.lock", 900),
        f("pyproject.toml", 2),
        f("docs/img/diagram.png", 0),
    ];
    assert_eq!(
        review_selection(&files, 15_000),
        [
            "pydantic/fields.py",
            "pydantic/json_schema.py",
            "pydantic/_known_annotated_metadata.py",
            "pyproject.toml"
        ]
    );
    // A tight budget leads with the files that fit whole; the larger
    // ones follow (read through continuePatch), never dropped.
    assert_eq!(
        review_selection(&files, 2_000),
        [
            "pydantic/json_schema.py",
            "pydantic/fields.py",
            "pydantic/_known_annotated_metadata.py",
            "pyproject.toml"
        ]
    );
    assert!(review_selection(&[f("tests/a_test.py", 3)], 15_000).is_empty());
    // Tests named by their language's convention stay out of the review
    // pick wherever they live.
    let polyglot = vec![
        f("pkg/cmd/discussion/client/client_test.go", 3_842),
        f("pkg/cmd/discussion/create/create.go", 120),
        f("src/test/java/com/acme/WidgetTest.java", 300),
        f("src/main/java/com/acme/Latest.java", 10),
        f("spec/models/user_spec.rb", 80),
        f("app/test_api.py", 60),
    ];
    assert_eq!(
        review_selection(&polyglot, 15_000),
        [
            "pkg/cmd/discussion/create/create.go",
            "src/main/java/com/acme/Latest.java"
        ]
    );
    // A literal search fills the page share without the metadata reserve.
    assert_eq!(literal_patch_window(None), 16_000);
    assert_eq!(literal_patch_window(Some(20_000)), 16_000);
}

#[test]
fn changed_file_globs_do_not_widen_literal_paths_or_case() {
    use super::super::super::filter::PathPattern;
    for (pattern, yes, no) in [
        ("src", "src", vec!["src/a.rs", "other/src"]),
        ("*.rs", "lib.rs", vec!["src/lib.rs", "LIB.RS"]),
        ("src/**", "src/deep/lib.rs", vec!["other/lib.rs"]),
    ] {
        let matcher = PathPattern::parse(pattern).expect("glob");
        assert!(matcher.matches(yes));
        for path in no {
            assert!(!matcher.matches(path), "{pattern} widened to {path}");
        }
    }
}

/// The review pick names every qualifying file its `include` lead can
/// carry: the contract's `include` bound, not a fixed count of its own.
#[test]
fn review_selection_names_every_file_the_include_bound_admits() {
    let max = crate::contracts::query_limits::gh_get_history_item::INCLUDE_MAX_ITEMS;
    let files = (0..max + 20)
        .map(|n| json!({"filename": format!("src/f{n:03}.rs"), "additions": 1, "deletions": 0}))
        .collect::<Vec<_>>();
    assert_eq!(review_selection(&files[..40], 1_000_000).len(), 40);
    assert_eq!(review_selection(&files, 1_000_000).len(), max);
}
