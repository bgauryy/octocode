use super::{StructureResult, StructureSearchQuery, execute_structure};
use crate::tools::cancel::NeverCancel;
use crate::tools::test_support::{AfterRoot, Fixture, sensitive_fixture, simple_policy};
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde_json::{Value, json};

/// Tests speak JSON rows, or a continuation's complete input (its one row);
/// the runtime owns the typed parse.
fn execute_row(
    query: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> StructureResult {
    let row = query
        .get("queries")
        .map_or(query.clone(), |rows| rows[0].clone());
    let query: StructureSearchQuery =
        serde_json::from_value(row).expect("typed structureSearch row");
    execute_structure(&query, paths, security, cancellation, None)
}

fn run(root: &std::path::Path, query: Value) -> StructureResult {
    let (paths, security) = simple_policy(root);
    execute_row(query, &paths, &security, &NeverCancel)
}

/// `files` rows expanded, one object per entry: `path` (`data.path` +
/// group `dir` + name, `.` naming the directory itself; bare entries are
/// `path`'s own), `type` for directories and symlinks, and the entry's
/// `size`, `lineCount` and `modifiedMs` fields.
fn listed(out: &Value) -> Vec<Value> {
    let root = out["path"].as_str().expect("path").to_owned();
    let join = |dir: &str| {
        if dir.is_empty() {
            root.clone()
        } else {
            format!("{root}/{dir}")
        }
    };
    out["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files groups: {out}"))
        .iter()
        .flat_map(|group| match group.as_str() {
            Some(entry) => vec![listed_entry(&root, entry)],
            None => {
                let dir = join(group["dir"].as_str().expect("dir"));
                group["files"]
                    .as_array()
                    .expect("group entries")
                    .iter()
                    .map(|entry| listed_entry(&dir, entry.as_str().expect("entry text")))
                    .collect()
            }
        })
        .collect()
}

fn listed_entry(dir: &str, entry: &str) -> Value {
    let (name, fields) = match entry.strip_suffix(')').and_then(|e| e.rsplit_once(" (")) {
        Some((name, fields)) => (name, fields.split(", ").collect::<Vec<_>>()),
        None => (entry, Vec::new()),
    };
    let mut row = json!({});
    let name = match name.strip_suffix('/') {
        Some(name) => {
            row["type"] = json!("directory");
            name
        }
        None => name,
    };
    row["path"] = json!(match (dir, name) {
        (dir, ".") => dir.to_owned(),
        ("", name) => name.to_owned(),
        (dir, name) => format!("{dir}/{name}"),
    });
    for field in fields {
        if field == "symlink" {
            row["type"] = json!("symlink");
        } else if let Some(lines) = field.strip_prefix("lineCount=") {
            row["lineCount"] = json!(lines.parse::<u64>().expect("lineCount"));
        } else if let Some(modified) = field.strip_prefix("modifiedMs=") {
            row["modifiedMs"] = json!(modified.parse::<i64>().expect("modifiedMs"));
        } else {
            row["size"] = json!(field.parse::<i64>().expect("size"));
        }
    }
    row
}

#[test]
fn invalid_time_filters_are_rejected_instead_of_skipped() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source\n").expect("source");
    for field in ["modifiedWithin", "modifiedBefore", "accessedWithin"] {
        let error = run(
            &root.0,
            json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"time":{field:"banana"}}),
        )
        .expect_err("invalid filter must not broaden the result");
        assert_eq!(error.code, "invalidInput");
        assert!(error.message.contains(field), "{}", error.message);
    }
}

#[test]
fn invalid_size_filter_is_invalid_input_not_an_execution_failure() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"size":{"greater":"10zz"}}),
    )
    .expect_err("an unparsable size must be rejected");
    assert_eq!(error.code, "invalidInput", "{error:?}");
    assert!(error.message.contains("10zz"), "{}", error.message);
}

#[test]
fn missing_path_is_not_found() {
    let root = Fixture::new();
    let error = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0.join("nope")}),
    )
    .expect_err("missing path");
    assert_eq!(error.code, "pathNotFound", "{error:?}");
}

/// A file passed as a tree root is named for what it is, workspace-relative
/// (no absolute path), with a lead that reads its outline instead.
#[test]
fn a_file_tree_root_leads_to_reading_the_file() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "fn a() {}\n").expect("file");
    {
        let error = run(
            &root.0,
            json!({"operation":"tree","path":root.0.join("src/lib.rs")}),
        )
        .expect_err("a file is not a directory");
        assert_eq!(error.code, "notADirectory", "{error:?}");
        assert!(
            !error.message.contains(&*root.0.to_string_lossy()),
            "absolute path leaked: {error:?}"
        );
        let lead = &error.next.as_ref().expect("lead")["read"];
        assert_eq!(lead["tool"], "localFetch", "{error:?}");
        let row = &lead["query"]["queries"][0];
        assert_eq!(row["path"], "src/lib.rs", "{error:?}");
        assert_eq!(row["minify"], "symbols", "{error:?}");
    }
}

/// A missing path leads to a tree of its nearest existing parent, so the
/// agent sees what is there instead of a dead end.
#[test]
fn a_missing_path_leads_to_its_nearest_existing_parent() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    let error = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src/gone/deeper")}),
    )
    .expect_err("missing path");
    assert_eq!(error.code, "pathNotFound", "{error:?}");
    let lead = &error.next.as_ref().expect("lead")["viewTree"];
    assert_eq!(lead["tool"], "structureSearch", "{lead}");
    let query = lead["query"]["queries"][0].clone();
    assert_eq!(query["operation"], "tree", "{lead}");
    assert_eq!(query["path"], "src", "{lead}");
    run(&root.0, query).expect("the parent lists");
}

#[test]
fn modified_and_full_detail_return_the_file_modification_time() {
    let root = Fixture::new();
    let source = root.0.join("source.rs");
    std::fs::write(&source, "source\n").expect("source");
    let modified = source
        .metadata()
        .expect("metadata")
        .modified()
        .expect("modified")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("epoch")
        .as_millis() as f64;
    for detail in ["basic", "modified", "full"] {
        let out = run(&root.0, json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","detail":detail,"sort":"name"})).expect("files");
        if detail == "basic" {
            assert!(listed(&out)[0].get("modifiedMs").is_none());
        } else {
            assert!(listed(&out)[0]["modifiedMs"].is_i64(), "{out}");
            let actual = listed(&out)[0]["modifiedMs"].as_f64().expect("modifiedMs");
            assert!((actual - modified).abs() < 1.0, "{out}");
        }
    }
}

#[test]
fn line_sort_and_full_counts_work_above_two_thousand_entries() {
    let root = Fixture::new();
    for i in 0..2001 {
        std::fs::write(root.0.join(format!("file-{i:04}.rs")), "line\n").expect("file");
    }
    std::fs::write(root.0.join("largest.rs"), "line\n".repeat(7)).expect("largest");
    let out = run(&root.0, json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","detail":"full","sort":"lines","pageSize":1})).expect("files");
    assert_eq!(out["pagination"]["totalItems"], 2002);
    assert!(
        listed(&out)[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/largest.rs")),
        "{out}"
    );
    assert_eq!(listed(&out)[0]["lineCount"], 7, "{out}");
}

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = sensitive_fixture();
    let files = run(
        &root.0,
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"detail":"full","sort":"lines","entryType":"f"}),
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalItems"], 1);
    assert_eq!(listed(&files)[0]["lineCount"], 2);
    assert!(!files.to_string().contains("credentials"));
    assert!(!files.to_string().contains("tfstate"));
    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"hidden":true}),
    )
    .expect("tree");
    assert_eq!(tree["entries"], json!(["visible.rs (21)"]), "{tree}");
}

#[test]
fn cancellation_interrupts_descendant_traversal() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source").expect("source");
    let (paths, security) = simple_policy(&root.0);
    for operation in ["files", "tree"] {
        let error = execute_row(
            json!({"operation":operation,"mainGoal": "test", "reasoning":"test","path":root.0}),
            &paths,
            &security,
            &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
        )
        .expect_err("cancel during walk");
        assert_eq!(error.code, "structure.execution.cancelled", "{operation}");
    }
}

#[cfg(unix)]
#[test]
fn escaped_links_are_pruned_before_line_counting() {
    let root = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.0.join("outside.rs"), "outside\n").expect("outside");
    std::os::unix::fs::symlink(outside.0.join("outside.rs"), root.0.join("link.rs")).expect("link");
    let (paths, security) = simple_policy(&root.0);
    let files = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"detail":"full","entryType":"f"}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalItems"], 0);
}

#[test]
fn files_path_sort_is_lexicographic_and_stable() {
    let root = Fixture::new();
    for name in ["zebra.rs", "apple.rs", "mango.rs", "banana.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let run = || {
        let out = execute_row(
            json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","sort":"path"}),
            &paths,
            &security,
            &NeverCancel,
        )
        .expect("files");
        listed(&out)
            .iter()
            .map(|f| f["path"].as_str().expect("path").to_string())
            .collect::<Vec<_>>()
    };
    let first = run();
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted, "path sort must be lexicographic");
    let second = run();
    assert_eq!(first, second, "path sort must be stable across runs");
}

#[test]
fn files_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    // Happy path: the freshly emitted snapshot must be accepted on page 2.
    let good = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("good page2");
    assert!(
        good.get("errorCode").is_none(),
        "valid continuation must not be rejected; got {good}"
    );
    assert_eq!(good["pagination"]["currentPage"], json!(2));
    std::fs::write(root.0.join("newcomer.rs"), "x\n").expect("mutate corpus");
    let page2 = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page2");
    assert_eq!(page2["errorCode"], json!("staleSnapshot"), "got {page2}");
    let restart = &page2["next"]["restart"];
    assert_eq!(restart["tool"], "structureSearch");
    assert_eq!(restart["query"]["queries"][0]["page"], 1);
    assert!(restart["query"]["queries"][0].get("snapshot").is_none());
    assert_eq!(restart["query"]["queries"][0]["entryType"], "f");
    let fresh = execute_row(
        restart["query"]["queries"][0].clone(),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("execute returned restart unchanged");
    assert_eq!(fresh["pagination"]["currentPage"], 1);
    assert_eq!(fresh["pagination"]["totalItems"], 7);
    assert_ne!(fresh["snapshot"], page1["snapshot"]);
}

/// A continuation that names its root relative to the workspace (as the
/// response envelope spells it) continues the same snapshot; the page number
/// travels once, in `next.nextPage.query`.
#[test]
fn a_workspace_relative_continuation_keeps_its_snapshot() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("sub")).expect("dir");
    for i in 0..6 {
        std::fs::write(root.0.join(format!("sub/f{i}.rs")), "x\n").expect("file");
    }
    for operation in ["tree", "files"] {
        let first = run(&root.0, json!({"operation":operation,"path":root.0.join("sub"),"pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first");
        assert!(
            first["pagination"].get("nextPage").is_none(),
            "{operation}: {first}"
        );
        let mut relative = first["next"]["nextPage"]["query"]["queries"][0].clone();
        relative["path"] = json!("sub");
        let second = run(&root.0, relative).expect("second");
        assert!(second.get("errorCode").is_none(), "{operation}: {second}");
        assert_eq!(
            second["pagination"]["currentPage"], 2,
            "{operation}: {second}"
        );
    }
}

#[test]
fn continuation_snapshots_bind_page_size_and_rendered_metadata() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    for operation in ["tree", "files"] {
        let first = run(&root.0, json!({"operation":operation,"path":root.0,"pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first");
        let mut resized = first["next"]["nextPage"]["query"]["queries"][0].clone();
        resized["pageSize"] = json!(3);
        let rejected = run(&root.0, resized).expect("resized cursor");
        assert_eq!(
            rejected["errorCode"], "staleSnapshot",
            "{operation}: {rejected}"
        );
        let restarted = run(
            &root.0,
            rejected["next"]["restart"]["query"]["queries"][0].clone(),
        )
        .expect("restart");
        assert_eq!(restarted["pagination"]["currentPage"], 1);
    }
    let first = run(&root.0, json!({"operation":"files","path":root.0,"detail":"full","pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first files");
    std::fs::write(root.0.join("f3.rs"), "changed\nmetadata\n").expect("change listed evidence");
    let rejected = run(
        &root.0,
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("unchanged cursor");
    assert_eq!(rejected["errorCode"], "staleSnapshot", "{rejected}");
}

#[test]
fn files_disclose_policy_withheld_coverage_without_exposing_names() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    let out = run(&root.0, json!({"operation":"files","path":root.0,"include":[".env.production"],"noIgnore":true,"defaultExcludes":false,"mainGoal":"test","reasoning":"test"})).expect("listing");
    assert!(listed(&out).is_empty(), "{out}");
    assert!(
        out["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|text| text.starts_with("1 entry withheld by path policy")))),
        "{out}"
    );
    assert!(
        !out["warnings"].to_string().contains(".env.production"),
        "{out}"
    );
}

#[test]
fn files_line_count_counts_lines_not_newlines_plus_one() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("two");
    std::fs::write(root.0.join("partial.rs"), "a\nb").expect("partial");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"detail":"full","entryType":"f"}),
    )
    .expect("files");
    let count = |name: &str| {
        listed(&out)
            .iter()
            .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with(name)))
            .map(|f| f["lineCount"].clone())
            .unwrap_or_else(|| panic!("{name} in {out}"))
    };
    assert_eq!(count("two.rs"), 2);
    assert_eq!(count("partial.rs"), 2);
    // Zero-line files omit lineCount rather than reporting an extra line.
    assert!(count("empty.rs").is_null());
}

#[test]
fn file_rows_omit_the_default_file_type() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    std::fs::write(root.0.join("dir/a.rs"), "fn a() {}\n").expect("a");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"sort":"path"}),
    )
    .expect("files");
    let rows = listed(&out);
    let file = rows
        .iter()
        .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with("a.rs")))
        .expect("file row");
    assert!(file.get("type").is_none(), "{file}");
    if let Some(dir) = rows
        .iter()
        .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with("/dir")))
    {
        assert_eq!(dir["type"], "directory", "{dir}");
    }
}

#[test]
fn tree_outlines_in_path_order_with_bounded_depth() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/deep")).expect("dirs");
    std::fs::write(root.0.join("src/deep/leaf.rs"), "x").expect("leaf");
    std::fs::write(root.0.join("src/lib.rs"), "xy").expect("lib");
    std::fs::write(root.0.join("README.md"), "").expect("readme");
    let top = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":1}),
    )
    .expect("top");
    assert_eq!(top["entries"], json!(["README.md (0)", "src/"]), "{top}");
    assert!(top.get("pagination").is_none(), "{top}");
    let all = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":6}),
    )
    .expect("all");
    // Entries of `path` itself are bare; a subdirectory's entries share one
    // `{dir, entries}` group (dir relative to `path`), directories in walk
    // order, so no path prefix repeats.
    assert_eq!(
        all["entries"],
        json!([
            "README.md (0)",
            {"dir":"src","entries":["lib.rs (2)"]},
            {"dir":"src/deep","entries":["leaf.rs (1)"]}
        ]),
        "{all}"
    );
    assert_eq!(all["summary"], "3 files, 2 dirs, 3B");
    let dirs = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":6,"entryType":"d"}),
    )
    .expect("dirs");
    assert_eq!(
        dirs["entries"],
        json!([{"dir":"src","entries":["deep/"]}]),
        "{dirs}"
    );
}

/// Groups follow the walk's directory order (`a/x` before `a-b`), keep each
/// directory's entries in walk order, and name a parent the filters left out.
#[test]
fn tree_groups_follow_walk_order_and_name_filtered_parents() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/x")).expect("dirs");
    std::fs::create_dir_all(root.0.join("a-b")).expect("dirs");
    std::fs::write(root.0.join("a/x/k.rs"), "x").expect("file");
    std::fs::write(root.0.join("a/m.rs"), "x").expect("file");
    std::fs::write(root.0.join("a-b/n.rs"), "x").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":4}),
    )
    .expect("tree");
    assert_eq!(
        out["entries"],
        json!([
            {"dir":"a","entries":["m.rs (1)"]},
            {"dir":"a/x","entries":["k.rs (1)"]},
            {"dir":"a-b","entries":["n.rs (1)"]}
        ]),
        "{out}"
    );
    let mut public = json!({"results":[{"index":0,"data":out}]});
    crate::response::continuations::finalize(
        &mut public,
        crate::tools::id::ToolId::StructureSearch,
        &crate::response::continuations::Sources::Rows(&[]),
        &crate::response::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    crate::contracts::validate_output("structureSearch", &public).expect("contract tree groups");
    let rs = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":4,"extensions":["rs"]}),
    )
    .expect("filtered tree");
    assert_eq!(
        rs["entries"],
        json!([
            {"dir":"a","entries":["m.rs (1)"]},
            {"dir":"a/x","entries":["k.rs (1)"]},
            {"dir":"a-b","entries":["n.rs (1)"]}
        ]),
        "{rs}"
    );
}

/// A page that continues a group repeats its `dir`, so each page reads on
/// its own and every entry arrives exactly once.
#[test]
fn a_tree_group_split_across_pages_repeats_its_dir() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("d")).expect("dir");
    for name in ["x", "y", "z"] {
        std::fs::write(root.0.join(format!("d/{name}")), "x").expect("file");
    }
    let page1 = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"pageSize":2,"maxDepth":2}),
    )
    .expect("page1");
    assert_eq!(
        page1["entries"],
        json!([{"dir":"d","entries":["x (1)","y (1)"]}]),
        "{page1}"
    );
    let page2 = run(
        &root.0,
        page1["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page2");
    assert_eq!(
        page2["entries"],
        json!([{"dir":"d","entries":["z (1)"]}]),
        "{page2}"
    );
    assert!(page2.get("next").is_none(), "{page2}");
}

#[test]
fn tree_pages_through_next_and_rejects_stale_snapshots() {
    let root = Fixture::new();
    for i in 0..5 {
        std::fs::write(root.0.join(format!("f{i}.txt")), "x").expect("file");
    }
    let page1 = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"pageSize":2}),
    )
    .expect("page1");
    assert_eq!(page1["pagination"]["totalPages"], 3, "{page1}");
    let next = &page1["next"]["nextPage"];
    assert_eq!(next["tool"], "structureSearch");
    assert_eq!(next["query"]["queries"][0]["operation"], "tree");
    let page2 = run(&root.0, next["query"]["queries"][0].clone()).expect("page2");
    assert_eq!(
        page2["entries"],
        json!(["f2.txt (1)", "f3.txt (1)"]),
        "{page2}"
    );
    // The walk summary (counts, hidden/ignored disclosure) rides page 1 only.
    assert!(page1.get("summary").is_some(), "{page1}");
    assert!(page2.get("summary").is_none(), "{page2}");
    std::fs::write(root.0.join("f9.txt"), "x").expect("mutate");
    let stale = run(&root.0, next["query"]["queries"][0].clone()).expect("stale");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    let restart = &stale["next"]["restart"];
    assert_eq!(restart["tool"], "structureSearch");
    assert_eq!(restart["query"]["queries"][0]["operation"], "tree");
    assert_eq!(restart["query"]["queries"][0]["page"], 1);
    assert!(restart["query"]["queries"][0].get("snapshot").is_none());
    let fresh = run(&root.0, restart["query"]["queries"][0].clone())
        .expect("execute returned restart unchanged");
    assert_eq!(fresh["pagination"]["currentPage"], 1);
    assert_eq!(fresh["pagination"]["totalItems"], 6);
    assert_ne!(fresh["snapshot"], page1["snapshot"]);
}

#[test]
fn old_ast_shapes_are_not_structure_queries() {
    let root = Fixture::new();
    for retired in [
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","treeKind":"filesystem","path":root.0}),
        json!({"operation":"syntaxTree","mainGoal": "test", "reasoning":"test","path":root.0}),
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"language":"rust"}),
    ] {
        assert!(
            serde_json::from_value::<StructureSearchQuery>(retired).is_err(),
            "not a structureSearch query"
        );
    }
}

/// Layout questions must never pay for (or depend on) a parser: the module
/// walks the filesystem only.
#[test]
fn structure_search_never_reaches_a_parser() {
    for (name, source) in [
        ("mod.rs", include_str!("mod.rs")),
        ("files.rs", include_str!("files.rs")),
        ("tree.rs", include_str!("tree.rs")),
    ] {
        for forbidden in [
            "structural::",
            "ast_search",
            "syntax_tree",
            "tree_sitter",
            "ast_grep",
            "language_extensions",
        ] {
            assert!(!source.contains(forbidden), "{name} references {forbidden}");
        }
    }
}

/// The tree leaves out what localSearch leaves out (gitignored entries,
/// unless noIgnore) and says how many sensitive entries the path policy
/// withheld instead of dropping them silently.
#[test]
fn tree_follows_gitignore_and_reports_withheld_sensitive_entries() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "Cargo.lock\n").expect("gitignore");
    for name in ["Cargo.lock", "main.rs", ".env.production", ".npmrc"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let names = |out: &Value| {
        out["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .filter_map(|entry| entry.as_str()?.split(' ').next().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":root.0,"hidden":true}),
    )
    .expect("tree");
    assert_eq!(names(&out), [".gitignore", "main.rs"], "{out}");
    // `.git`, `.env.production`, `.npmrc`: named by policy, not by path.
    assert_eq!(
        out["warnings"][0],
        "3 entries withheld by path policy: security-policy dirs .git/; 2 credential files. No flag or config setting lifts it; absence there is unproven.",
        "{out}"
    );
    let all = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":root.0,"hidden":true,"noIgnore":true}),
    )
    .expect("tree");
    assert_eq!(
        names(&all),
        [".gitignore", "Cargo.lock", "main.rs"],
        "{all}"
    );
    // Without `hidden`, dot entries are out of view and not counted.
    let plain = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":root.0}),
    )
    .expect("tree");
    assert!(!plain.to_string().contains("withheld"), "{plain}");
}

/// `files` leaves out `.gitignore`d entries like the tree and localSearch, and
/// prunes ignored directories during the walk instead of listing their
/// contents; `defaultExcludes:false` walks everything.
#[test]
fn files_follows_gitignore_and_prunes_ignored_directories() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "vendor/\n*.node\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("vendor/pkg")).expect("vendor");
    std::fs::create_dir_all(root.0.join("app")).expect("app");
    for name in [
        "package.json",
        "app/package.json",
        "vendor/pkg/package.json",
        "addon.node",
    ] {
        std::fs::write(root.0.join(name), "{}\n").expect("file");
    }
    let paths = |out: &Value| {
        let mut paths = listed(out)
            .iter()
            .filter_map(|row| row["path"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"include":["package.json","*.node"]}),
    )
    .expect("files");
    let base = root.0.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        paths(&out),
        [
            format!("{base}/app/package.json"),
            format!("{base}/package.json")
        ],
        "{out}"
    );
    let all = run(
        &root.0,
        json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"include":["package.json","*.node"],"defaultExcludes":false}),
    )
    .expect("files");
    assert_eq!(paths(&all).len(), 4, "{all}");
}

/// A path whose children are all `.gitignore`d is not empty for the caller's
/// reasons: say ignore rules dropped the entries and offer an executable
/// retry that includes them, instead of the generic "broaden" fallback.
#[test]
fn all_ignored_listing_names_the_ignore_rules_and_offers_a_retry() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "repos/*\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("repos/java/src")).expect("repos");
    std::fs::write(root.0.join("repos/java/src/A.java"), "class A {}\n").expect("file");
    std::fs::write(root.0.join("repos/notes.txt"), "x\n").expect("file");
    let repos = root.0.join("repos");

    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":repos}),
    )
    .expect("tree");
    assert_eq!(tree["status"], "empty", "{tree}");
    let hints = tree["hints"].as_array().expect("hints");
    assert!(
        hints.iter().any(|hint| hint
            .as_str()
            .is_some_and(|hint| hint.contains(".gitignore") && hint.contains("noIgnore"))),
        "{tree}"
    );
    let retry = &tree["next"]["includeIgnored"];
    assert_eq!(retry["tool"], "structureSearch", "{tree}");
    assert_eq!(retry["query"]["queries"][0]["noIgnore"], true, "{tree}");
    assert!(
        retry["query"]["queries"][0].get("snapshot").is_none(),
        "{tree}"
    );
    let retried = run(&root.0, retry["query"]["queries"][0].clone()).expect("retry");
    assert!(
        retried["entries"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{retried}"
    );

    let files = run(
        &root.0,
        json!({"operation":"files","mainGoal":"test","reasoning":"test","path":repos,"extensions":["java"]}),
    )
    .expect("files");
    assert_eq!(files["status"], "empty", "{files}");
    assert!(
        files["hints"]
            .as_array()
            .is_some_and(|hints| hints.iter().any(|hint| hint
                .as_str()
                .is_some_and(|hint| hint.contains(".gitignore") && hint.contains("noIgnore")))),
        "{files}"
    );
    let retry = &files["next"]["includeIgnored"];
    assert_eq!(retry["query"]["queries"][0]["noIgnore"], true, "{files}");
    let found = run(&root.0, retry["query"]["queries"][0].clone()).expect("retry");
    assert_eq!(listed(&found).len(), 1, "{found}");
}

/// A genuinely empty directory keeps the generic empty result: no ignore
/// rule dropped anything, so no ignore hint or retry is offered.
#[test]
fn empty_listing_without_ignored_entries_offers_no_ignore_retry() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("empty")).expect("dir");
    let empty = root.0.join("empty");
    for query in [
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":empty}),
        json!({"operation":"files","mainGoal":"test","reasoning":"test","path":empty,"extensions":["java"]}),
    ] {
        let out = run(&root.0, query).expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        assert!(out.get("hints").is_none(), "{out}");
        assert!(out["next"].get("includeIgnored").is_none(), "{out}");
    }
}

/// A non-empty tree still says how many entries `.gitignore` hid, so a
/// directory of ignored checkouts next to one tracked README does not read
/// as a directory holding only the README.
#[test]
fn tree_summary_counts_gitignored_entries_it_left_out() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "repos/*\n!repos/README.md\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("repos/java")).expect("repos");
    std::fs::create_dir_all(root.0.join("repos/go")).expect("repos");
    std::fs::write(root.0.join("repos/README.md"), "# repos\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":root.0.join("repos")}),
    )
    .expect("tree");
    assert_eq!(out["entries"].as_array().map(Vec::len), Some(1), "{out}");
    assert!(
        out["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("2 entries hidden by .gitignore")),
        "{out}"
    );
    assert!(out.get("hints").is_none(), "{out}");
    let all = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":root.0.join("repos"),"noIgnore":true}),
    )
    .expect("tree");
    assert!(
        !all["summary"]
            .as_str()
            .unwrap_or_default()
            .contains(".gitignore"),
        "{all}"
    );
}

/// A path-ordered listing is the walk order, so the walk stops once the
/// requested limit is filled instead of scanning the whole tree.
#[test]
fn path_sorted_listing_stops_walking_at_the_limit() {
    let root = Fixture::new();
    for dir in ["a", "a-b", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..10 {
            std::fs::write(root.0.join(format!("{dir}/f{n}.go")), "x\n").expect("file");
        }
    }
    let listing = |limit: Option<u32>| {
        let mut query = json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"extensions":["go"],"sort":"path"});
        if let Some(limit) = limit {
            query["maxEntries"] = json!(limit);
        }
        run(&root.0, query).expect("files")
    };
    let full = listing(None);
    let prefix = format!("{}/", root.0.file_name().unwrap().to_string_lossy());
    let names = |out: &Value| {
        listed(out)
            .iter()
            .map(|row| {
                let path = row["path"].as_str().expect("path");
                path.strip_prefix(&prefix).unwrap_or(path).to_owned()
            })
            .collect::<Vec<_>>()
    };
    let all = names(&full);
    assert_eq!(all.len(), 30);
    // Walk order: a directory's contents follow it (`a/…` before `a-b/…`).
    assert_eq!(all[9], "a/f9.go");
    assert_eq!(all[10], "a-b/f0.go");
    assert!(full.get("truncated").is_none(), "{full}");
    let cut = listing(Some(3));
    assert_eq!(names(&cut), all[..3], "{cut}");
    assert_eq!(cut["truncated"], true);
    assert_eq!(cut["partialReasons"], json!(["maxEntries"]));
    // The walk stopped early: the total is a lower bound, not a count.
    assert!(cut.get("totalAvailable").is_none(), "{cut}");
    assert_eq!(cut["atLeast"], 4, "{cut}");
    assert!(cut.get("terminalLimit").is_none(), "{cut}");
    assert_eq!(
        cut["next"]["expandScan"]["query"]["queries"][0]["maxEntries"], 6,
        "{cut}"
    );
}

/// `tree` takes the same name filter as `files`; `files` takes `noIgnore`
/// like `tree` (gitignore only, unlike defaultExcludes:false).
#[test]
fn tree_filters_names_and_files_lists_ignored_entries_on_request() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "gen.rs\n").expect("gitignore");
    std::fs::write(root.0.join("a_test.rs"), "x\n").expect("a");
    std::fs::write(root.0.join("lib.rs"), "x\n").expect("lib");
    std::fs::write(root.0.join("gen.rs"), "x\n").expect("gen");
    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"include":["*_test.rs"]}),
    )
    .expect("tree");
    assert_eq!(tree["entries"], json!(["a_test.rs (2)"]), "{tree}");
    let prefix = format!("{}/", root.0.file_name().unwrap().to_string_lossy());
    let files = |extra: Value| {
        let mut query = json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"extensions":["rs"]});
        query
            .as_object_mut()
            .expect("query")
            .extend(extra.as_object().expect("extra").clone());
        let out = run(&root.0, query).expect("files");
        listed(&out)
            .iter()
            .map(|row| {
                let path = row["path"].as_str().expect("path");
                path.strip_prefix(&prefix).unwrap_or(path).to_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(files(json!({})), ["a_test.rs", "lib.rs"]);
    assert_eq!(
        files(json!({"noIgnore":true})),
        ["a_test.rs", "gen.rs", "lib.rs"]
    );
}

/// With no sort, files are listed in path (git ls-files) order.
#[test]
fn files_default_to_path_order() {
    let root = Fixture::new();
    for name in ["b.rs", "a.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"entryType":"f"}),
    )
    .expect("files");
    let base = root.0.file_name().unwrap().to_string_lossy().into_owned();
    let paths = listed(&out)
        .iter()
        .map(|row| row["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            format!("{base}/a.rs"),
            format!("{base}/b.rs"),
            format!("{base}/c.rs")
        ],
        "{out}"
    );
}

#[test]
fn a_default_listing_is_one_page_within_the_budget_and_budget_pages_beyond_it() {
    let root = Fixture::new();
    for n in 0..150 {
        std::fs::write(root.0.join(format!("module_{n:03}.py")), "x = 1\n").expect("source");
    }
    let query = json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"include":["*.py"]});
    let out = run(&root.0, query.clone()).expect("files");
    assert_eq!(listed(&out).len(), 150, "{out}");
    assert_eq!(out["pagination"]["hasMore"], false, "{out}");
    assert!(out["pagination"].get("pageSize").is_none(), "{out}");
    assert!(out["next"].get("nextPage").is_none(), "{out}");
    // Rows carry the byte count; the formatted size is a debug field.
    assert_eq!(listed(&out)[0]["size"], 6, "{out}");

    // A caller page size still pages by count.
    let mut sized = query.clone();
    sized["pageSize"] = json!(100);
    let out = run(&root.0, sized).expect("sized files");
    assert_eq!(listed(&out).len(), 100, "{out}");
    assert_eq!(out["pagination"]["pageSize"], 100, "{out}");
    assert!(out["next"]["nextPage"].is_object(), "{out}");

    // Past the budget, following nextPage reaches every row exactly once.
    for n in 150..4000 {
        std::fs::write(root.0.join(format!("module_{n:04}.py")), "x = 1\n").expect("source");
    }
    let mut next = query;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let out = run(&root.0, next.clone()).expect("page");
        pages += 1;
        for row in listed(&out) {
            assert!(seen.insert(row["path"].as_str().expect("path").to_owned()));
        }
        let Some(call) = out["next"].get("nextPage") else {
            break;
        };
        assert!(
            call["query"]["queries"][0].get("pageSize").is_none(),
            "{call}"
        );
        next = call["query"]["queries"][0].clone();
    }
    assert_eq!(seen.len(), 4000);
    assert!((3..=6).contains(&pages), "{pages} pages");
}

/// `files` rows group consecutive entries by directory: each entry is its
/// name (`/` after a directory, `.` for the walked root itself) and its
/// ` (<size>[, …])` fields, so `dir` + `/` + name is the row path and the
/// listing keeps its order, every entry once.
#[test]
fn files_rows_group_by_directory_in_listing_order() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("sub/deep")).expect("dirs");
    std::fs::write(root.0.join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.0.join("sub/b.rs"), "b\n").expect("b");
    std::fs::write(root.0.join("sub/deep/c (1).rs"), "c\nc\n").expect("c");
    std::fs::write(root.0.join("z.rs"), "z\n").expect("z");
    let name = root.0.file_name().unwrap().to_string_lossy().into_owned();
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"sort":"path"}),
    )
    .expect("files");
    assert_eq!(
        out["files"],
        json!([
            "a.rs (10)",
            "z.rs (2)",
            {"dir": "sub", "files": ["b.rs (2)"]},
            {"dir": "sub/deep", "files": ["c (1).rs (4)"]}
        ]),
        "{out}"
    );
    // The directories' own groups name them; they are not listed twice.
    assert_eq!(out["pagination"]["totalItems"], 4, "{out}");
    let paths = listed(&out)
        .iter()
        .map(|row| row["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            format!("{name}/a.rs"),
            format!("{name}/z.rs"),
            format!("{name}/sub/b.rs"),
            format!("{name}/sub/deep/c (1).rs"),
        ]
    );

    let full = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0.join("sub/deep"),"detail":"full"}),
    )
    .expect("full");
    let entry = full["files"][0].as_str().expect("entry");
    assert!(
        entry.starts_with("c (1).rs (4, lineCount=2, modifiedMs="),
        "{full}"
    );
    let row = &listed(&full)[0];
    assert_eq!(row["size"], 4);
    assert_eq!(row["lineCount"], 2);
    assert!(row["modifiedMs"].is_i64(), "{row}");

    // A file root lists its own name, bare.
    let file = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0.join("a.rs")}),
    )
    .expect("file root");
    assert_eq!(file["files"], json!(["a.rs (10)"]));
}

/// A page that continues a directory group repeats its `dir`, so each page
/// reads on its own, and the pages hold every entry exactly once.
#[test]
fn a_group_split_across_pages_repeats_its_directory_on_each_page() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    for n in 0..5 {
        std::fs::write(root.0.join(format!("src/f{n}.rs")), "x\n").expect("file");
    }
    let mut query = json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"entryType":"f","pageSize":2});
    let mut pages = Vec::new();
    loop {
        let out = run(&root.0, query.clone()).expect("page");
        pages.push(out["files"].clone());
        let Some(next) = out["next"].get("nextPage") else {
            break;
        };
        query = next["query"]["queries"][0].clone();
    }
    assert_eq!(
        pages,
        [
            json!([{"dir": "src", "files": ["f0.rs (2)", "f1.rs (2)"]}]),
            json!([{"dir": "src", "files": ["f2.rs (2)", "f3.rs (2)"]}]),
            json!([{"dir": "src", "files": ["f4.rs (2)"]}]),
        ]
    );
}

/// A budget page counts a group header when a row opens a group: on a new
/// directory, and again on the first row of every page.
#[test]
fn budget_pages_count_a_header_for_each_group_a_page_opens() {
    use super::{RowCost, page_ranges};
    let row = |continues| RowCost {
        entry: 10,
        header: 25,
        continues,
    };
    // One group of six rows: 35 + 10 + 10 = 55 fits 60; the fourth row
    // would make 65, so it opens the next page, header included.
    let costs = [false, true, true, true, true, true].map(row);
    assert_eq!(page_ranges(&costs, None, 60), [0..3, 3..6]);
    // A new group mid-page costs its header too: 35 + 35 > 60.
    let costs = [false, false, true].map(row);
    assert_eq!(page_ranges(&costs, None, 60), [0..1, 1..3]);
    // A caller page size pages by count.
    assert_eq!(page_ranges(&costs, Some(2), 0), [0..2, 2..3]);
}

/// A tree without `hidden` skips dot entries, but says how many it skipped
/// and offers the executable retry that includes them; pruned directories
/// such as `.git` stay out of the count, since `hidden:true` prunes them too.
#[test]
fn tree_discloses_skipped_dot_entries_and_offers_include_hidden() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git");
    std::fs::create_dir_all(root.0.join(".github")).expect("github");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join(".editorconfig"), "x\n").expect("dotfile");
    std::fs::write(root.0.join("src/.keep"), "").expect("nested dotfile");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":2}),
    )
    .expect("tree");
    assert_eq!(
        out["entries"],
        json!([{"dir":"src","entries":["lib.rs (2)"]}]),
        "{out}"
    );
    // An unfiltered outline names the count; the retry lead is for a
    // filtered or empty listing.
    assert!(
        out["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("3 dot entries skipped")),
        "{out}"
    );
    assert!(out["next"].get("includeHidden").is_none(), "{out}");
    let filtered = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":2,"extensions":["rs"]}),
    )
    .expect("filtered tree");
    let retry = &filtered["next"]["includeHidden"];
    assert_eq!(retry["tool"], "structureSearch", "{out}");
    assert_eq!(retry["query"]["queries"][0]["hidden"], true, "{out}");
    assert!(
        retry["query"]["queries"][0].get("snapshot").is_none(),
        "{out}"
    );
    let all = run(&root.0, retry["query"]["queries"][0].clone()).expect("retry");
    assert_eq!(
        all["entries"],
        json!([{"dir":"src","entries":["lib.rs (2)"]}]),
        "{all}"
    );
    assert!(all["next"].get("includeHidden").is_none(), "{all}");
    assert!(
        !all["summary"].as_str().unwrap_or_default().contains("dot"),
        "{all}"
    );
}

/// A `files` result's first page leads to its first outline-able file: a
/// localFetch `minify:"symbols"` read of its declarations. A listing of
/// only non-source files offers none, and later pages repeat none.
#[test]
fn a_files_result_leads_to_the_top_files_outline() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/blob.bin"), [0_u8, 1, 2]).expect("bin");
    std::fs::write(root.0.join("src/lib.rs"), "pub fn a() {}\n").expect("lib");
    std::fs::write(root.0.join("src/main.rs"), "fn main() {}\n").expect("main");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.rs","*.bin"]}),
    )
    .expect("files");
    let lead = &out["next"]["read"];
    assert_eq!(lead["tool"], "localFetch", "{out}");
    let path = lead["query"]["queries"][0]["path"].as_str().expect("path");
    assert!(path.ends_with("src/lib.rs"), "{out}");
    assert!(std::path::Path::new(path).is_absolute(), "{out}");
    assert_eq!(lead["query"]["queries"][0]["minify"], "symbols", "{out}");
    assert_eq!(
        lead["query"]["queries"][0].as_object().map(|q| q.len()),
        Some(2),
        "{out}"
    );
    crate::contracts::validate_query("localFetch", lead["query"]["queries"][0].clone())
        .expect("read is a valid localFetch query");

    for name in ["c.rs", "d.rs", "e.rs"] {
        std::fs::write(root.0.join("src").join(name), "fn x() {}\n").expect("more");
    }
    let wide = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.rs","*.bin"],"pageSize":2}),
    )
    .expect("files");
    assert!(wide.pointer("/next/read").is_some(), "{wide}");
    let later = run(
        &root.0,
        wide["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page 2");
    assert!(later.pointer("/next/read").is_none(), "{later}");
    let binary = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.bin"]}),
    )
    .expect("files");
    assert!(binary.pointer("/next/read").is_none(), "{binary}");
}

/// A directory listed by its own `{dir, entries}` group on the same page is
/// not also listed as a `"name/"` entry of its parent; an empty or
/// depth-cut directory (no group) keeps its `"name/"` entry.
#[test]
fn a_tree_directory_with_a_group_is_not_listed_twice() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/inner")).expect("dirs");
    std::fs::create_dir(root.0.join("empty")).expect("empty");
    std::fs::write(root.0.join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.0.join("src/lib.rs"), "fn b() {}\n").expect("lib");
    std::fs::write(root.0.join("src/inner/deep.rs"), "fn c() {}\n").expect("deep");
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"maxDepth":2}),
    )
    .expect("tree");
    let entries = out["entries"].as_array().expect("entries");
    let bare = entries.iter().filter_map(Value::as_str).collect::<Vec<_>>();
    assert!(bare.contains(&"empty/"), "{out}");
    assert!(!bare.contains(&"src/"), "{out}");
    let src = entries
        .iter()
        .find(|entry| entry["dir"] == "src")
        .expect("src group");
    // `src/inner` has no group at maxDepth 2, so it stays listed.
    assert!(
        src["entries"]
            .as_array()
            .unwrap()
            .contains(&json!("inner/")),
        "{out}"
    );
}

/// `files` groups like `tree`: entries of `path` itself are bare strings,
/// each subdirectory is one `{dir, files}` group with `dir` relative to
/// `path` (walk order, merged), and a directory whose group is on the page
/// is not also listed as `"name/"`.
#[test]
fn files_group_like_tree_relative_to_path() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("a/x.rs"), "x").expect("x");
    std::fs::write(root.0.join("a/b/y.rs"), "y").expect("y");
    std::fs::write(root.0.join("a/z.rs"), "z").expect("z");
    std::fs::write(root.0.join("top.rs"), "t").expect("top");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["*.rs","a","b"]}),
    )
    .expect("files");
    assert_eq!(
        out["files"],
        json!([
            "top.rs (1)",
            {"dir":"a","files":["x.rs (1)","z.rs (1)"]},
            {"dir":"a/b","files":["y.rs (1)"]}
        ]),
        "{out}"
    );
}

/// Default-pruned directories are disclosed: the tree summary names them,
/// and an empty listing leads to the same listing with them walked.
#[test]
fn default_pruned_directories_are_named_and_an_empty_listing_walks_them() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("build/gen")).expect("build");
    std::fs::create_dir_all(root.0.join("node_modules/pkg")).expect("deps");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("build/gen/electron.ts"), "x\n").expect("generated");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");

    let tree = run(&root.0, json!({"operation":"tree","path":root.0})).expect("tree");
    let summary = tree["summary"].as_str().expect("summary");
    assert!(
        summary.ends_with("2 default-excluded dirs not walked: build, node_modules"),
        "{tree}"
    );
    assert!(tree["next"].get("includeIgnored").is_none(), "{tree}");

    let files = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"]}),
    )
    .expect("files");
    assert_eq!(files["status"], "empty", "{files}");
    assert!(
        files["hints"].to_string().contains("default-excluded"),
        "{files}"
    );
    let retry = &files["next"]["includeIgnored"]["query"]["queries"][0];
    assert_eq!(retry["defaultExcludes"], false, "{files}");
    assert!(retry.get("snapshot").is_none(), "{files}");
    let found = run(&root.0, retry.clone()).expect("retry");
    assert_eq!(listed(&found).len(), 1, "{found}");
}

/// The policy-withheld warning only counts entries the query's filters
/// could have listed.
#[test]
fn withheld_entries_count_only_when_the_filters_could_list_them() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    std::fs::write(root.0.join("main.go"), "package main\n").expect("go");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["go"],"noIgnore":true}),
    )
    .expect("listing");
    assert_eq!(listed(&out).len(), 1, "{out}");
    assert!(!out.to_string().contains("withheld"), "{out}");
}

/// A directory listed under its own group is never also listed bare under
/// its parent, even when the two land on different pages: every entry is
/// shown exactly once across the pages.
#[test]
fn a_grouped_directory_is_listed_once_across_pages() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("sub")).expect("sub");
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("root file");
    }
    std::fs::write(root.0.join("sub/d.rs"), "x\n").expect("sub file");
    for operation in ["tree", "files"] {
        let mut query = json!({"operation":operation,"path":root.0,"maxDepth":2,"pageSize":2});
        let mut shown = Vec::new();
        loop {
            let page = run(&root.0, query.clone()).expect("page");
            shown.push(
                page[if operation == "tree" {
                    "entries"
                } else {
                    "files"
                }]
                .to_string(),
            );
            match page["next"]["nextPage"]["query"]["queries"].get(0) {
                Some(next) => query = next.clone(),
                None => break,
            }
        }
        let all = shown.join("");
        assert!(!all.contains("\"sub/\""), "{operation}: {all}");
        assert_eq!(all.matches("d.rs").count(), 1, "{operation}: {all}");
    }
}

/// An omitted `maxDepth` lists `path`'s children; an `include` filter then
/// searches every level, and a bare word matches names containing it.
#[test]
fn tree_depth_defaults_to_children_and_include_searches_every_level() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b/c")).expect("dirs");
    std::fs::write(root.0.join("a/b/c/deep_helper.rs"), "x\n").expect("deep");
    std::fs::write(root.0.join("top.rs"), "x\n").expect("top");
    let outline = run(&root.0, json!({"operation":"tree","path":root.0})).expect("tree");
    assert_eq!(outline["entries"], json!(["a/", "top.rs (2)"]), "{outline}");
    let found = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"include":["helper"]}),
    )
    .expect("include");
    assert!(found.to_string().contains("deep_helper.rs"), "{found}");
    assert!(!found.to_string().contains("top.rs"), "{found}");
}

/// Continuation pages reuse the first page's walk while nothing listed
/// changed, and still restart when a listed file changes.
#[test]
fn continuation_pages_reuse_the_walk_and_restart_on_change() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let first = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType":"f","pageSize":2}),
    )
    .expect("page 1");
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    let second = run(&root.0, next.clone()).expect("page 2");
    assert_eq!(second["pagination"]["currentPage"], 2, "{second}");
    let third = run(
        &root.0,
        second["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page 3");
    assert_eq!(third["pagination"]["currentPage"], 3, "{third}");
    std::fs::write(root.0.join("f5.rs"), "changed size\n").expect("change");
    let stale = run(&root.0, next).expect("stale page");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
}

/// `maxDepth` counts the same in both operations: 1 lists `path`'s
/// children, 2 their children too.
#[test]
fn files_and_tree_count_max_depth_alike() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("top.rs"), "x\n").expect("top");
    std::fs::write(root.0.join("a/mid.rs"), "x\n").expect("mid");
    std::fs::write(root.0.join("a/b/low.rs"), "x\n").expect("low");
    for (depth, expected) in [(1, vec!["top.rs"]), (2, vec!["mid.rs", "top.rs"])] {
        for operation in ["tree", "files"] {
            let out = run(
                &root.0,
                json!({"operation":operation,"path":root.0,"maxDepth":depth,"extensions":["rs"]}),
            )
            .expect("listing");
            let text = out.to_string();
            for name in ["top.rs", "mid.rs", "low.rs"] {
                assert_eq!(
                    text.contains(name),
                    expected.contains(&name),
                    "{operation} maxDepth {depth} {name}: {out}"
                );
            }
        }
    }
}

/// A listing cut at the largest `maxEntries` keeps every listed row
/// reachable: each page with `hasMore` carries `next.nextPage`, and the
/// walk ceiling is a disclosed terminal limit with a narrowing lead, not a
/// silent end of paging.
#[test]
fn files_at_the_walk_ceiling_page_every_listed_row_and_lead_to_narrow() {
    let root = Fixture::new();
    for dir in 0..11 {
        let dir_path = root.0.join(format!("d{dir:02}"));
        std::fs::create_dir_all(&dir_path).expect("fixture dir");
        for file in 0..1_000 {
            std::fs::write(dir_path.join(format!("f{file:04}.ts")), "").expect("fixture file");
        }
    }
    let mut query = json!({"operation":"files","path":root.0,"extensions":["ts"],"pageSize":1000});
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let out = run(&root.0, query.clone()).expect("files page");
        pages += 1;
        for row in listed(&out) {
            assert!(seen.insert(row["path"].to_string()), "duplicate {row}");
        }
        if out["pagination"]["hasMore"] == true {
            assert!(
                out["next"]["nextPage"].is_object(),
                "hasMore without next: {}",
                out["pagination"]
            );
        }
        assert_eq!(out["terminalLimit"], true, "{}", out["pagination"]);
        let Some(next) = out["next"].get("nextPage") else {
            let narrow = &out["next"]["narrowScope"]["query"]["queries"][0];
            assert_eq!(narrow["operation"], "tree", "{out}");
            assert_eq!(narrow["maxDepth"], 1, "{out}");
            assert!(
                out["warnings"].to_string().contains("are on no page"),
                "{}",
                out["warnings"]
            );
            break;
        };
        query = next["query"].clone();
        assert!(pages < 20, "runaway paging");
    }
    assert_eq!(seen.len(), 10_000);
    assert!(pages > 1);
}

/// SS3: the files read lead skips generated and large files.
#[test]
fn files_read_lead_skips_generated_and_large() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a_types.generated.rs"),
        "pub struct A;\n".repeat(8_000),
    )
    .expect("generated");
    std::fs::write(root.0.join("b.rs"), "fn b() {}\n").expect("small");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    let read = out["next"]["read"]["query"]["queries"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("read lead: {out}"));
    assert!(read.ends_with("b.rs"), "{out}");
}

/// SS3: a root tree leads to its manifest, read whole.
#[test]
fn root_tree_leads_to_manifest() {
    let root = Fixture::new();
    std::fs::write(root.0.join("package.json"), "{\"name\":\"x\"}\n").expect("manifest");
    std::fs::write(root.0.join("README.md"), "# x\n").expect("readme");
    std::fs::create_dir(root.0.join("src")).expect("src");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("tree");
    let query = &out["next"]["read"]["query"]["queries"][0];
    assert!(
        query["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("package.json")),
        "{out}"
    );
    assert!(query.get("minify").is_none(), "{out}");
}

/// SS4: tree and files print the same exact byte size.
#[test]
fn tree_and_files_print_exact_byte_sizes() {
    let root = Fixture::new();
    std::fs::write(root.0.join("f.txt"), "x".repeat(9216)).expect("file");
    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("tree");
    let files = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    assert_eq!(tree["entries"], json!(["f.txt (9216)"]), "{tree}");
    assert_eq!(files["files"], json!(["f.txt (9216)"]), "{files}");
}

/// SS4: a directory root is the listing, never one of its rows.
#[test]
fn files_never_lists_root_dot() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "a\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    assert!(!out["files"].to_string().contains("\"./\""), "{out}");
    assert_eq!(out["files"], json!(["a.rs (2)"]), "{out}");
}

/// SS6: a gitignored directory named like a default-pruned one is lifted
/// in one hop: the retry sets both flags and lists its files.
#[test]
fn include_ignored_lead_lifts_gitignore_and_prune_in_one_hop() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("target/debug")).expect("target");
    std::fs::write(root.0.join("target/debug/x.rlib"), "rlib\n").expect("rlib");
    std::fs::write(root.0.join("main.rs"), "fn main() {}\n").expect("main");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"include":["*.rlib"]}),
    )
    .expect("files");
    assert_eq!(out["status"], "empty", "{out}");
    let retry = out["next"]["includeIgnored"]["query"]["queries"][0].clone();
    assert_eq!(retry["noIgnore"], true, "{out}");
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    let hint = out["hints"][0].as_str().expect("hint");
    assert!(hint.chars().count() <= 120, "{hint}");
    let found = run(&root.0, retry).expect("retry");
    assert!(found["files"].to_string().contains("x.rlib"), "{found}");
}

/// N8b: the tree summary states only the file and directory counts, and
/// totalItems counts the strings the pages show.
#[test]
fn tree_summary_matches_paged_row_counts() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("a/x.rs"), "x\n").expect("x");
    std::fs::write(root.0.join("a/b/y.rs"), "y\n").expect("y");
    std::fs::write(root.0.join("top.rs"), "t\n").expect("top");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":3,"pageSize":1}),
    )
    .expect("tree");
    assert_eq!(out["summary"], "3 files, 2 dirs, 6B", "{out}");
    let total = out["pagination"]["totalItems"].as_u64().expect("total");
    let mut strings = 0;
    let mut query = json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":3,"pageSize":1});
    loop {
        let page = run(&root.0, query.clone()).expect("page");
        for entry in page["entries"].as_array().expect("entries") {
            strings += entry
                .get("entries")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
        }
        match page["next"].get("nextPage") {
            Some(next) => query = next["query"]["queries"][0].clone(),
            None => break,
        }
    }
    assert_eq!(strings as u64, total, "{out}");
}

/// N8a: an empty files listing names withheld entries once (warnings).
#[test]
fn files_empty_withheld_not_duplicated() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("secrets")).expect("secrets");
    std::fs::write(root.0.join("secrets/a.ts"), "x\n").expect("secret");
    std::fs::write(root.0.join("b.rs"), "x\n").expect("other");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"extensions":["ts"]}),
    )
    .expect("files");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        out.to_string().matches("withheld by path policy").count(),
        1,
        "{out}"
    );
}

/// X7: a bare word include lists the files inside directories named by it.
#[test]
fn bare_word_include_lists_dir_files() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/inner")).expect("dirs");
    std::fs::write(root.0.join("src/inner/a.rs"), "a\n").expect("a");
    std::fs::write(root.0.join("other.rs"), "o\n").expect("other");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"include":["src"]}),
    )
    .expect("files");
    let text = out["files"].to_string();
    assert!(text.contains("a.rs"), "{out}");
    assert!(!text.contains("other.rs"), "{out}");
}
