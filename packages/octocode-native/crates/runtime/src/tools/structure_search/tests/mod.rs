use super::{StructureResult, StructureSearchQuery, execute_structure};
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{AfterRoot, Fixture, sensitive_fixture, simple_policy};
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

mod budget;
mod ignore;
mod listing;
mod roles;
mod scans;

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

/// Whether `value` holds a string equal to `text` at any depth: a listing
/// entry, exactly as printed.
fn has_string(value: &Value, text: &str) -> bool {
    match value {
        Value::String(item) => item == text,
        Value::Array(items) => items.iter().any(|item| has_string(item, text)),
        Value::Object(fields) => fields.values().any(|item| has_string(item, text)),
        _ => false,
    }
}

/// Runs `start` and every page lead its pages offer (`checks::walk`: each
/// call once, each listed entry once, every page and lead within the
/// contract). Returns the raw pages in walk order.
fn walk_pages(root: &std::path::Path, start: Value) -> Vec<Value> {
    let mut pages = Vec::new();
    crate::tools::test_support::checks::walk(
        "structureSearch",
        json!({"queries": [start]}),
        |call| {
            let row = &call["queries"][0];
            let out = run(root, row.clone()).unwrap_or_else(|error| panic!("{row}: {error:?}"));
            let page = crate::tools::test_support::checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::StructureSearch,
                    row,
                    &out,
                ),
                rows: listed(&out)
                    .iter()
                    .map(|entry| entry["path"].as_str().expect("path").to_owned())
                    .collect(),
            };
            pages.push(out);
            page
        },
    );
    pages
}

/// `files` rows expanded, one object per entry: `path` (`data.path` +
/// group `dir` + name, `.` naming the directory itself; bare entries are
/// `path`'s own), `type` for directories and symlinks, and the entry's
/// `size`, `totalLines` and `modifiedMs` fields.
fn listed(out: &Value) -> Vec<Value> {
    let root = out["path"].as_str().expect("path").to_owned();
    // Group dirs are workspace-relative; fixtures list the workspace
    // root itself (`.`) or a direct child of it (named like `root`).
    let join = |dir: &str| {
        if dir.is_empty() || dir == "." {
            root.clone()
        } else if dir == root || dir.starts_with(&format!("{root}/")) {
            dir.to_owned()
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
        } else if matches!(field, "binary" | "unreadable") {
            row["totalLines"] = json!(field);
        } else if let Some(lines) = field.strip_prefix("totalLines=") {
            row["totalLines"] = json!(lines.parse::<u64>().expect("totalLines"));
        } else if let Some(modified) = field.strip_prefix("modifiedMs=") {
            row["modifiedMs"] = json!(modified.parse::<i64>().expect("modifiedMs"));
        } else {
            row["size"] = json!(field.parse::<i64>().expect("size"));
        }
    }
    row
}

#[test]
fn full_detail_counts_lines_only_for_the_selected_page() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "one\ntwo\n").expect("source");
    }
    let before = super::rows::line_count_reads();
    let first = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extras":["details"],"sort":"name","maxEntries":4,"pageSize":1}),
    )
    .expect("listing");
    assert_eq!(listed(&first).len(), 1);
    assert_eq!(super::rows::line_count_reads() - before, 1);
}

#[test]
fn invalid_time_filters_are_rejected_instead_of_skipped() {
    // Admission rejects malformed duration strings before generated typed
    // parsing or the walk; a typed executor test cannot construct this query.
    for field in ["modifiedWithin", "modifiedBefore", "accessedWithin"] {
        let error = crate::contracts::validate_query(
            "structureSearch",
            json!({"operation":"files","path":".","time":{field:"banana"}}),
        )
        .expect_err("invalid filter must not broaden the result");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path == ["time", field]),
            "{field}: {error:?}"
        );
        let diagnostic = crate::contracts::format_input_error("structureSearch", &error);
        assert!(diagnostic.to_string().contains(field), "{diagnostic}");
    }
}

/// A duration the contract's pattern admits but no walk can apply (it
/// overflows) fails as invalid input instead of listing unfiltered.
#[test]
fn an_overflowing_time_filter_is_invalid_input_not_an_unfiltered_listing() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() {}\n");
    for field in ["modifiedWithin", "modifiedBefore", "accessedWithin"] {
        let error = run(
            &root.0,
            json!({"operation":"files","path":root.0,"time":{field:"99999999999999999999d"}}),
        )
        .expect_err("an overflowing duration must not broaden the result");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{field}: {error:?}");
        assert!(
            error.message.contains(&format!("time.{field}=")),
            "{error:?}"
        );
    }
}

#[test]
fn invalid_size_filter_is_invalid_input_not_an_execution_failure() {
    // The contract's `size` pattern rejects an unreadable bound before the
    // walk runs, and names the field.
    for (size, field) in [
        (json!({"greater":"10zz"}), "size.greater"),
        (json!({"greater":"1.5k","less":"k"}), "size.less"),
    ] {
        let error = crate::contracts::validate_query(
            "structureSearch",
            json!({"operation":"files","path":".","size":size}),
        )
        .expect_err("an unparsable size is invalid input");
        let paths: Vec<String> = error
            .issues
            .iter()
            .map(|issue| issue.path.join("."))
            .collect();
        assert!(
            paths.iter().any(|path| path.ends_with(field)),
            "{field}: {paths:?}"
        );
    }
    for valid in ["10k", "1.5m", "200", "3gb"] {
        crate::contracts::validate_query(
            "structureSearch",
            json!({"operation":"files","path":".","size":{"greater":valid}}),
        )
        .unwrap_or_else(|error| panic!("{valid}: {error:?}"));
    }
}

#[test]
fn missing_path_is_not_found() {
    let root = Fixture::new();
    let error = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0.join("nope")}),
    )
    .expect_err("missing path");
    assert_eq!(error.code, ErrorCode::PathNotFound, "{error:?}");
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
        assert_eq!(error.code, ErrorCode::NotADirectory, "{error:?}");
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

/// A path refusal with no shared code (here a symlink loop) uses the local
/// tools' one code, never a structureSearch-only dotted code.
#[cfg(unix)]
#[test]
fn a_symlink_loop_root_is_a_path_validation_failure() {
    let root = Fixture::new();
    std::os::unix::fs::symlink(root.0.join("b"), root.0.join("a")).expect("a");
    std::os::unix::fs::symlink(root.0.join("a"), root.0.join("b")).expect("b");
    let error = run(&root.0, json!({"operation":"tree","path":root.0.join("a")}))
        .expect_err("a looping root");
    assert_eq!(error.code, ErrorCode::PathValidationFailed, "{error:?}");
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
    assert_eq!(error.code, ErrorCode::PathNotFound, "{error:?}");
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
    for details in [false, true] {
        let extras: &[&str] = if details { &["details"] } else { &[] };
        let out = run(&root.0, json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","extras":extras,"sort":"name"})).expect("files");
        if !details {
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
    let out = run(&root.0, json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","extras":["details"],"sort":"lines","pageSize":1})).expect("files");
    assert_eq!(out["pagination"]["totalItems"], 2002);
    assert!(
        listed(&out)[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/largest.rs")),
        "{out}"
    );
    assert_eq!(listed(&out)[0]["totalLines"], 7, "{out}");
}

/// A list sorted by lines shows the value it is sorted by, with no extras:
/// each file row carries its `totalLines`, in descending order.
#[test]
fn a_line_sort_shows_the_line_counts_without_extras() {
    let root = Fixture::new();
    std::fs::write(root.0.join("big.rs"), "line\n".repeat(9)).expect("big");
    std::fs::write(root.0.join("small.rs"), "line\n".repeat(2)).expect("small");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"sort":"lines"}),
    )
    .expect("files");
    let rows = listed(&out);
    let lines = rows
        .iter()
        .filter(|row| row["type"] != "directory")
        .map(|row| row["totalLines"].clone())
        .collect::<Vec<_>>();
    assert_eq!(lines, [json!(9), json!(2), json!(0)], "{out}");
    let by_size = run(
        &root.0,
        json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"entryType":"f","sort":"size"}),
    )
    .expect("files");
    assert!(listed(&by_size)[0].get("totalLines").is_none(), "{by_size}");
}

/// A line sort counts text files only: a binary file has no lines (it
/// says `binary`) and an unreadable one an unknown count (`unreadable`),
/// never a count of 0 that reads like an empty file; both rank after the
/// counted files.
#[test]
fn a_line_sort_names_binary_and_unreadable_files_instead_of_counting_them() {
    let root = Fixture::new();
    std::fs::write(root.0.join("text.rs"), "a\nb\n").expect("text");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let mut binary = vec![b'\n'; 64];
    binary.extend([0_u8, 1, 2, 3]);
    std::fs::write(root.0.join("blob.bin"), &binary).expect("binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(root.0.join("locked.rs"), "a\n").expect("locked");
        std::fs::set_permissions(
            root.0.join("locked.rs"),
            std::fs::Permissions::from_mode(0o000),
        )
        .expect("chmod");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType":"f","sort":"lines"}),
    )
    .expect("files");
    let lines = listed(&out)
        .iter()
        .map(|row| {
            let path = row["path"].as_str().expect("path");
            (
                path.rsplit('/').next().expect("name").to_owned(),
                row["totalLines"].clone(),
            )
        })
        .collect::<Vec<_>>();
    let expected = vec![
        ("text.rs".to_owned(), json!(2)),
        ("empty.rs".to_owned(), json!(0)),
        ("blob.bin".to_owned(), json!("binary")),
    ];
    // Root reads every file whatever its mode.
    #[cfg(unix)]
    let expected = {
        let mut expected = expected;
        if std::fs::read(root.0.join("locked.rs")).is_err() {
            expected.push(("locked.rs".to_owned(), json!("unreadable")));
        } else {
            expected.insert(1, ("locked.rs".to_owned(), json!(1)));
        }
        expected
    };
    assert_eq!(lines, expected, "{out}");
}

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = sensitive_fixture();
    let files = run(
        &root.0,
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"extras":["details"],"sort":"lines","entryType":"f"}),
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalItems"], 1);
    assert_eq!(listed(&files)[0]["totalLines"], 2);
    crate::tools::test_support::checks::assert_absent(&files, "credentials");
    crate::tools::test_support::checks::assert_absent(&files, "tfstate");
    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"includeHidden":true}),
    )
    .expect("tree");
    assert_eq!(
        tree["files"],
        json!([{"dir":".","files":["visible.rs (21)"]}]),
        "{tree}"
    );
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
        assert_eq!(error.code, ErrorCode::Cancelled, "{operation}");
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
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"extras":["details"],"entryType":"f"}),
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
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(
            restart["query"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {restart}"
        );
    }
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
    let first = run(&root.0, json!({"operation":"files","path":root.0,"extras":["details"],"pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first files");
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
    let out = run(&root.0, json!({"operation":"files","path":root.0,"include":[".env.production"],"includeIgnored":true,"includeHidden":true,"defaultExcludes":false,"mainGoal":"test","reasoning":"test"})).expect("listing");
    assert!(listed(&out).is_empty(), "{out}");
    assert!(
        out["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|text| text.starts_with("1 entry withheld by path policy")))),
        "{out}"
    );
    crate::tools::test_support::checks::assert_absent(&out["warnings"], ".env.production");
}

#[test]
fn files_line_count_counts_lines_not_newlines_plus_one() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("two");
    std::fs::write(root.0.join("partial.rs"), "a\nb").expect("partial");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"extras":["details"],"entryType":"f"}),
    )
    .expect("files");
    let count = |name: &str| {
        listed(&out)
            .iter()
            .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with(name)))
            .map(|f| f["totalLines"].clone())
            .unwrap_or_else(|| panic!("{name} in {out}"))
    };
    assert_eq!(count("two.rs"), 2);
    assert_eq!(count("partial.rs"), 2);
    // Zero-line files omit totalLines rather than reporting an extra line.
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

/// `tree` and `files` count a symlink alike in `summary`: a non-directory
/// entry of its own size.
#[cfg(unix)]
#[test]
fn tree_and_files_summaries_count_symlinks_alike() {
    let root = Fixture::new();
    std::fs::write(root.0.join("target.md"), "abc").expect("target");
    std::fs::create_dir(root.0.join("links")).expect("dir");
    for name in ["a", "b"] {
        std::os::unix::fs::symlink("../target.md", root.0.join("links").join(name)).expect("link");
    }
    let summary = |operation: &str| {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0.join("links"),"extras":["details"]}),
        )
        .expect("listing");
        out["summary"].clone()
    };
    let tree = summary("tree");
    assert_eq!(tree["files"], 2, "{tree}");
    // A link's own length is no file content: bytes count regular files.
    assert_eq!(tree["bytes"], 0, "{tree}");
    assert_eq!(tree, summary("files"));
}

/// Both operations' `summary` totals the listing (every page of it, within
/// `maxEntries`), never entries the walk cut off.
#[test]
fn summaries_total_the_listing_not_the_whole_walk() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"] {
        std::fs::write(root.0.join(name), "xy").expect("file");
    }
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "maxEntries": 2, "entryType": "f"}),
        )
        .expect("listing");
        assert_eq!(
            out["summary"],
            json!({"files": 2, "dirs": 0, "bytes": 4}),
            "{operation}: {out}"
        );
    }
}

/// A tree entry has the one listing-entry shape `files` uses and the output
/// schema states: `extras: ["details"]` adds `totalLines=N` and
/// `modifiedMs=N` to each file, and a symlink is its own size plus the `symlink` field.
#[cfg(unix)]
#[test]
fn tree_entries_carry_details_and_symlink_fields_like_files() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("file");
    std::os::unix::fs::symlink(root.0.join("two.rs"), root.0.join("link.rs")).expect("link");
    let tree = |extras: Value| {
        run(
            &root.0,
            json!({"operation":"tree","path":root.0,"maxDepth":1,"extras":extras}),
        )
        .expect("tree")
    };
    let basic = tree(json!([]));
    assert!(
        basic["files"][0]["files"]
            .as_array()
            .is_some_and(|entries| entries.contains(&json!("two.rs (4)"))),
        "{basic}"
    );
    let link = basic["files"][0]["files"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .find(|entry| entry.starts_with("link.rs "))
        })
        .unwrap_or_else(|| panic!("link.rs entry: {basic}"));
    assert!(
        link.ends_with(", symlink)"),
        "the link's size, then symlink: {basic}"
    );
    let detailed = tree(json!(["details"]));
    let entry = detailed["files"][0]["files"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .find(|entry| entry.starts_with("two.rs "))
        })
        .unwrap_or_else(|| panic!("two.rs entry: {detailed}"));
    assert!(
        entry.starts_with("two.rs (4, totalLines=2, modifiedMs="),
        "{detailed}"
    );
}

/// The field a listing is sorted by is default output (L6-8): a modified
/// sort shows each file's `modifiedMs` without `extras`, as the lines sort
/// shows `totalLines`; another sort leaves it to `extras: ["details"]`.
#[test]
fn a_modified_sort_shows_its_key_without_extras() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("file");
    let entry = |sort: &str| {
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"entryType":"f","sort":sort}),
        )
        .expect("files");
        out["files"][0]["files"][0]
            .as_str()
            .unwrap_or_else(|| panic!("entry: {out}"))
            .to_owned()
    };
    assert!(entry("modified").starts_with("two.rs (4, modifiedMs="));
    assert_eq!(entry("size"), "two.rs (4)");
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
    assert_eq!(
        top["files"],
        json!([{"dir":".","files":["README.md (0)", "src/"]}]),
        "{top}"
    );
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
        all["files"],
        json!([{"dir":".","files":["README.md (0)"]},{"dir":"src","files":["lib.rs (2)"]},
            {"dir":"src/deep","files":["leaf.rs (1)"]}]),
        "{all}"
    );
    assert_eq!(all["summary"], json!({"files":3,"dirs":2,"bytes":3}));
    let dirs = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":6,"entryType":"d"}),
    )
    .expect("dirs");
    assert_eq!(
        dirs["files"],
        json!([{"dir":"src","files":["deep/"]}]),
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
        out["files"],
        json!([
            {"dir":"a","files":["m.rs (1)"]},
            {"dir":"a/x","files":["k.rs (1)"]},
            {"dir":"a-b","files":["n.rs (1)"]}
        ]),
        "{out}"
    );
    let mut public = json!({"results":[{"index":0,"data":out}]});
    crate::response::continuations::finalize(
        &mut public,
        crate::id::ToolId::StructureSearch,
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
        rs["files"],
        json!([
            {"dir":"a","files":["m.rs (1)"]},
            {"dir":"a/x","files":["k.rs (1)"]},
            {"dir":"a-b","files":["n.rs (1)"]}
        ]),
        "{rs}"
    );
}

/// Every page of a sorted, paged `files` listing (directories fold into
/// group headers, so the walk discovers more entries than it lists) carries
/// only declared pagination fields.
#[test]
fn sorted_paged_files_pages_satisfy_the_output_contract() {
    let root = Fixture::new();
    for dir in ["a/x", "b", "c"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dirs");
    }
    for file in ["a/x/k.rs", "a/m.rs", "b/n.rs", "c/o.rs", "p.rs"] {
        std::fs::write(root.0.join(file), "x\n").expect("file");
    }
    for sort in ["name", "size", "modified", "lines"] {
        let pages = walk_pages(
            &root.0,
            json!({"operation":"files","path":root.0,"sort":sort,"pageSize":2}),
        );
        assert!(pages.len() > 1, "sort {sort} pages");
    }
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
        page1["files"],
        json!([{"dir":"d","files":["x (1)","y (1)"]}]),
        "{page1}"
    );
    let page2 = run(
        &root.0,
        page1["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page2");
    assert_eq!(
        page2["files"],
        json!([{"dir":"d","files":["z (1)"]}]),
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
        page2["files"],
        json!([{"dir":".","files":["f2.txt (1)", "f3.txt (1)"]}]),
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
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(
            restart["query"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {restart}"
        );
    }
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
        ("mod.rs", include_str!("../mod.rs")),
        ("files.rs", include_str!("../files.rs")),
        ("tree.rs", include_str!("../tree.rs")),
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
