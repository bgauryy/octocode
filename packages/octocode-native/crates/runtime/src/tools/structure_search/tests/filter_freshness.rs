//! A metadata filter decides a listing by what it read of every entry it
//! evaluated, listed or not: an edit that moves any of them across the filter
//! restarts the walk, in every view and sort.
use super::*;
use crate::tools::test_support::Fixture;
use std::time::{Duration, SystemTime};

/// Follows `start`'s pages, `edit` applied after the first: the rows listed, or
/// the stale row the walk ended on.
fn walk_across_edit(
    root: &std::path::Path,
    start: Value,
    edit: impl FnOnce(),
) -> Result<Vec<String>, Value> {
    let first = run(root, start).expect("page 1");
    assert!(
        first["next"].get("nextPage").is_some(),
        "the walk has later pages: {first}"
    );
    let mut rows = paths(&first);
    let mut next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    // A file's time moves past the stamp the walk read.
    std::thread::sleep(Duration::from_millis(20));
    edit();
    loop {
        let out = run(root, next.clone()).expect("later page");
        if out["errorCode"] == "staleSnapshot" {
            return Err(out);
        }
        rows.extend(paths(&out));
        match out["next"].get("nextPage") {
            Some(lead) => next = lead["input"]["queries"][0].clone(),
            None => return Ok(rows),
        }
    }
}

/// The shapes a filter runs under: a view that shows only names, one that shows
/// metadata, and a sort that ranks by it.
fn shapes() -> [Value; 3] {
    [
        json!({"view":"paths"}),
        json!({"view":"details"}),
        json!({"view":"paths","sort":{"by":"sizeBytes","order":"desc"}}),
    ]
}

fn query(root: &std::path::Path, filter: &Value, shape: &Value) -> Value {
    let mut row = json!({"operation":"files","path":root,"limit":2});
    let fields = row.as_object_mut().expect("row");
    fields.extend(filter.as_object().expect("filter").clone());
    fields.extend(shape.as_object().expect("shape").clone());
    row
}

/// Eight files the filter keeps (`a`..`h`) and one it leaves out (`z`).
fn fixture(kept: &str, left_out: &str) -> Fixture {
    let root = Fixture::new();
    for name in "abcdefgh".chars() {
        root.write(&format!("{name}.txt"), kept);
    }
    root.write("z.txt", left_out);
    root
}

/// Each edit either way across `filter` ends the walk `staleSnapshot`.
fn every_edit_restarts(
    filter: &Value,
    fixture: &dyn Fn() -> Fixture,
    leaves: &dyn Fn(&std::path::Path),
    enters: &dyn Fn(&std::path::Path),
) {
    for shape in shapes() {
        for (name, edit) in [("leaves", leaves), ("enters", enters)] {
            let root = fixture();
            let walked =
                walk_across_edit(&root.0, query(&root.0, filter, &shape), || edit(&root.0));
            let stale = walked.unwrap_or_else(|out| {
                assert!(
                    out["related"][0]["input"]["queries"][0].is_object(),
                    "{out}"
                );
                vec!["stale".to_owned()]
            });
            assert_eq!(
                stale,
                ["stale"],
                "{filter} {shape}: an entry that {name} the filter"
            );
        }
    }
}

#[test]
fn a_size_filter_restarts_when_an_entry_moves_across_it() {
    every_edit_restarts(
        &json!({"sizeBytes":{"min":100}}),
        &|| fixture(&"x".repeat(200), "x"),
        // On a later page, whatever the sort: every kept file ties.
        &|root| std::fs::write(root.join("g.txt"), "x").expect("shrink"),
        &|root| std::fs::write(root.join("z.txt"), "x".repeat(300)).expect("grow"),
    );
}

#[test]
fn a_modified_time_filter_restarts_when_an_entry_moves_across_it() {
    let at = |path: std::path::PathBuf, time: SystemTime| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("file")
            .set_times(std::fs::FileTimes::new().set_modified(time))
            .expect("time");
    };
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    every_edit_restarts(
        &json!({"modifiedAt":{"from":"2020-01-01T00:00:00Z"}}),
        &|| {
            let root = fixture("x", "x");
            at(root.0.join("z.txt"), old);
            root
        },
        &|root| at(root.join("g.txt"), old),
        &|root| at(root.join("z.txt"), SystemTime::now()),
    );
}

#[test]
fn an_empty_filter_restarts_when_an_entry_moves_across_it() {
    every_edit_restarts(
        &json!({"empty":true}),
        &|| fixture("", "x"),
        &|root| std::fs::write(root.join("g.txt"), "x").expect("fill"),
        &|root| std::fs::write(root.join("z.txt"), "").expect("empty"),
    );
}

#[cfg(unix)]
#[test]
fn a_permission_filter_restarts_when_an_entry_moves_across_it() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: std::path::PathBuf, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    };
    let exact = || {
        let root = fixture("x", "x");
        for name in "abcdefgh".chars() {
            mode(root.0.join(format!("{name}.txt")), 0o640);
        }
        mode(root.0.join("z.txt"), 0o600);
        root
    };
    every_edit_restarts(
        &json!({"permissions":"640"}),
        &exact,
        &|root| mode(root.join("g.txt"), 0o600),
        &|root| mode(root.join("z.txt"), 0o640),
    );
    let executable = || {
        let root = fixture("x", "x");
        for name in "abcdefgh".chars() {
            mode(root.0.join(format!("{name}.txt")), 0o750);
        }
        mode(root.0.join("z.txt"), 0o640);
        root
    };
    every_edit_restarts(
        &json!({"access":"executable"}),
        &executable,
        &|root| mode(root.join("g.txt"), 0o640),
        &|root| mode(root.join("z.txt"), 0o750),
    );
}

/// An access time moves with no stamp to show it, so a filter on it keeps
/// nothing: each page walks, and the rows it finds decide.
#[test]
fn an_access_time_filter_decides_each_page_by_walking() {
    let root = fixture("x", "x");
    let start = query(
        &root.0,
        &json!({"accessedAt":{"from":"2000-01-01T00:00:00Z"}}),
        &json!({"view":"paths"}),
    );
    let walks = crate::tools::structure_search::listing::listing_walks();
    let pages = walk_pages(&root.0, start);
    assert!(pages.len() > 1, "{pages:?}");
    assert_eq!(
        crate::tools::structure_search::listing::listing_walks() - walks,
        pages.len(),
        "one walk per page"
    );
}

/// A row past the window a names-only filter walk is reading has no page proving
/// it yet: the next window answers for it, kept or walked.
#[test]
fn a_row_past_the_window_that_left_the_filter_restarts_the_walk() {
    for operation_sort in [json!({}), json!({"sort":{"by":"path","order":"asc"}})] {
        let root = fixture(&"x".repeat(200), "x");
        let start = query(&root.0, &json!({"sizeBytes":{"min":100}}), &operation_sort);
        let mut start = start;
        start["view"] = json!("paths");
        start.as_object_mut().expect("row").remove("limit");
        let walked = with_window(3, || {
            walk_across_edit(&root.0, start, || {
                std::fs::write(root.0.join("h.txt"), "x").expect("shrink");
            })
        });
        assert!(walked.is_err(), "{operation_sort}: {walked:?}");
    }
}

/// Nothing changed: the filter's rows are read whole, each once, on one walk.
#[test]
fn an_unchanged_filtered_walk_keeps_its_rows_and_its_walk() {
    for shape in shapes() {
        let root = fixture(&"x".repeat(200), "x");
        let start = query(&root.0, &json!({"sizeBytes":{"min":100}}), &shape);
        let walks = crate::tools::structure_search::listing::listing_walks();
        let pages = walk_pages(&root.0, start);
        let mut rows = pages.iter().flat_map(paths).collect::<Vec<_>>();
        rows.sort();
        assert_eq!(rows.len(), 8, "{shape}: {rows:?}");
        assert!(!rows.iter().any(|row| row.ends_with("z.txt")), "{rows:?}");
        assert_eq!(
            crate::tools::structure_search::listing::listing_walks() - walks,
            1,
            "{shape}: later pages reuse the walk"
        );
    }
}
