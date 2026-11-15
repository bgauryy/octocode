//! What one listing costs and what its stored walk watches: a page reads
//! nothing its walk already read, and a view that shows no metadata reads none.
use super::*;

fn stamp_stats() -> usize {
    octocode_engine::stamp::stamp_stats()
}

/// `dirs` directories of `files` files each.
fn tree(root: &Fixture, dirs: usize, files: usize) {
    for dir in 0..dirs {
        for file in 0..files {
            root.write(&format!("d{dir:03}/f{file:03}.rs"), "x\n");
        }
    }
}

/// A first page keeps its walk for the next one by the stamps the walk read:
/// the cost of keeping it does not grow with the rows, in any view.
#[test]
fn keeping_a_walk_stats_nothing_its_walk_read() {
    for view in ["paths", "details"] {
        let cost = |files| {
            let root = Fixture::new();
            tree(&root, 2, files);
            let query = json!({"operation":"files","path":root.0,"view":view});
            let before = stamp_stats();
            let out = run_budget(&root.0, query, 1).expect("page");
            assert!(out["next"].get("nextPage").is_some(), "{view}: {out}");
            stamp_stats() - before
        };
        assert_eq!(cost(4), cost(32), "{view}: stamps grew with the rows");
    }
}

/// A paths page shows names, so only the directories that hold them can
/// change it: serving the next page reads no listed file.
#[test]
fn a_stored_paths_walk_watches_its_directories_not_its_files() {
    let cost = |files| {
        let root = Fixture::new();
        tree(&root, 2, files);
        let query = json!({"operation":"files","path":root.0,"view":"paths"});
        let first = run_budget(&root.0, query, 1).expect("first");
        let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        let before = stamp_stats();
        let second = run_budget(&root.0, next, 1).expect("second");
        assert_ne!(second["errorCode"], "staleSnapshot", "{second}");
        stamp_stats() - before
    };
    assert_eq!(
        cost(4),
        cost(32),
        "serving a page grew with the files listed"
    );
}

/// A walk's stored rows depend on every directory it entered, not only those
/// that held a row: a new match in a directory that held none drops them.
#[test]
fn a_new_match_in_a_directory_with_no_listed_row_drops_the_stored_walk() {
    for view in ["paths", "details"] {
        let root = Fixture::new();
        tree(&root, 1, 8);
        root.write("other/readme.txt", "x\n");
        let query = json!({"operation":"files","path":root.0,"view":view,"extensions":["rs"]});
        let first = run_budget(&root.0, query, 1).expect("first");
        let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        root.write("other/new.rs", "x\n");
        // A directory's time can stay within its clock tick: moved explicitly.
        std::fs::File::open(root.0.join("other"))
            .and_then(|dir| dir.set_modified(std::time::SystemTime::UNIX_EPOCH))
            .expect("move the directory's time");
        let second = run_budget(&root.0, next, 1).expect("second");
        assert_eq!(second["errorCode"], "staleSnapshot", "{view}: {second}");
    }
}

/// A file whose size or time changed changes a details page, and a stored
/// details walk notices it; a paths page shows neither and keeps serving.
#[test]
fn an_edited_file_stales_a_details_walk_and_not_a_paths_walk() {
    for (view, stale) in [("details", true), ("paths", false)] {
        let root = Fixture::new();
        tree(&root, 1, 8);
        let query = json!({"operation":"files","path":root.0,"view":view});
        let first = run_budget(&root.0, query, 1).expect("first");
        let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        root.write("d000/f007.rs", "grown\ngrown\n");
        let second = run_budget(&root.0, next, 1).expect("second");
        assert_eq!(
            second["errorCode"] == "staleSnapshot",
            stale,
            "{view}: {second}"
        );
    }
}

/// A paths listing stats no row, so it states no byte total; details does.
#[test]
fn only_a_view_that_read_sizes_states_bytes() {
    let root = Fixture::new();
    root.write("a.rs", "12345");
    let summary = |view: &str| {
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"view":view}),
        )
        .expect("listing");
        out["summary"].clone()
    };
    assert_eq!(summary("details")["bytes"], 5);
    assert_eq!(summary("paths").get("bytes"), None);
    assert_eq!(summary("paths")["fileCount"], 1);
}

/// Large selected files stay in a paths listing without triggering an
/// advisory read or a size-based omission.
#[test]
fn a_paths_listing_keeps_large_files_without_read_advice() {
    let root = Fixture::new();
    for (name, size) in [("a.rs", 2 * 1024 * 1024), ("b.rs", 10)] {
        let file = std::fs::File::create(root.0.join(name)).expect("file");
        file.set_len(size).expect("size");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"view":"paths"}),
    )
    .expect("listing");
    let shown = out["files"].to_string();
    assert!(shown.contains("a.rs") && shown.contains("b.rs"), "{out}");
    assert!(related(&out, "localFetch").is_null(), "{out}");
}

/// Every listed directory is a `name/` row of its parent's group, once, also
/// when it heads a `{dir, files}` group of its own: the header alone would
/// not say the directory was selected. Every file stays listed.
#[test]
fn a_paths_listing_lists_each_directory_as_a_row_once() {
    let root = Fixture::new();
    root.write("top.rs", "x");
    root.write("a/one.rs", "x");
    root.write("a/b/two.rs", "x");
    root.write("only/mid/leaf.rs", "x");
    std::fs::create_dir(root.0.join("empty")).expect("empty");
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"view":"paths","maxDepth":5}),
        )
        .expect("listing");
        let mut rows = Vec::new();
        for group in out["files"].as_array().expect("groups") {
            let dir = group["dir"].as_str().expect("dir");
            for entry in group["files"].as_array().expect("entries") {
                let entry = entry.as_str().expect("entry");
                if let Some(name) = entry.strip_suffix('/') {
                    let at = std::path::Path::new(dir).join(name);
                    let at = at.strip_prefix(&root.0).unwrap_or(&at);
                    rows.push(at.to_string_lossy().trim_start_matches("./").to_owned());
                }
            }
        }
        rows.sort();
        assert_eq!(
            rows,
            ["a", "a/b", "empty", "only", "only/mid"],
            "{operation}: {out}"
        );
        assert_eq!(out["summary"]["dirCount"], 5, "{operation}: {out}");
    }
}

fn ranked(root: &Fixture, by: &str, order: &str) -> Vec<String> {
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"view":"paths",
               "defaultExcludes":false,"sort":{"by":by,"order":order}}),
    )
    .expect("listing");
    out["files"]
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|group| group["files"].as_array().expect("entries"))
        .map(|entry| entry.as_str().expect("entry").to_owned())
        .collect()
}

/// A rank's key is its first evidence: entries without one (a directory or a
/// link has no content size; a directory, binary file or unreadable one no
/// line count) come after the ones that have it in both directions, so the
/// first page answers "smallest" as it answers "largest".
#[test]
fn entries_without_the_sort_key_rank_last_in_both_directions() {
    let root = Fixture::new();
    root.write("big.rs", "1\n2\n3\n");
    root.write("small.rs", "1\n");
    std::fs::write(root.0.join("data.bin"), b"\0\0\0").expect("binary");
    std::fs::create_dir(root.0.join("zdir")).expect("dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.0.join("big.rs"), root.0.join("zlink")).expect("link");
    let last = |rows: &[String], name: &str| rows.iter().position(|row| row.starts_with(name));
    for order in ["asc", "desc"] {
        let sized = ranked(&root, "sizeBytes", order);
        let dir = last(&sized, "zdir").expect("directory listed");
        assert!(dir >= 3, "{order}: {sized:?}");
        #[cfg(unix)]
        assert!(
            last(&sized, "zlink").expect("link listed") >= 3,
            "{order}: {sized:?}"
        );
        let lined = ranked(&root, "totalLines", order);
        assert!(
            last(&lined, "zdir").expect("dir") >= 2,
            "{order}: {lined:?}"
        );
        assert!(
            last(&lined, "data.bin").expect("bin") >= 2,
            "{order}: {lined:?}"
        );
    }
    assert!(ranked(&root, "sizeBytes", "asc")[0].starts_with("small.rs"));
    assert!(ranked(&root, "sizeBytes", "desc")[0].starts_with("big.rs"));
    assert!(ranked(&root, "totalLines", "asc")[0].starts_with("small.rs"));
    assert!(ranked(&root, "totalLines", "desc")[0].starts_with("big.rs"));
}

/// Whole-file test code decides a listing: a file whose text cannot hold a
/// whole-file `cfg` or a parent's mount of it is settled without a parse.
#[test]
fn a_test_code_listing_parses_only_what_could_change_its_verdict() {
    let parses = |modules: usize| {
        let root = Fixture::new();
        let mut lib = String::new();
        for module in 0..modules {
            lib.push_str(&format!("mod m{module};\n"));
            root.write(
                &format!("src/m{module}.rs"),
                "pub fn run() {}\n#[cfg(test)]\nmod inner {\n    #[test]\n    fn t() {}\n}\n",
            );
        }
        // Other modules are declared after a `cfg` too; only the gated one
        // asks for a parse.
        lib.push_str(
            "#[cfg(test)]\nmod helpers;\n#[cfg(test)]\n// note; see\n/* a;\n*/\nmod noted;\n",
        );
        lib.push_str("#[cfg(test)]\n#[doc = \"a;b\"]\nmod docced;\nmod after;\npub fn run() {}\n");
        root.write("src/noted.rs", "pub fn fixture() {}\n");
        root.write("src/docced.rs", "pub fn fixture() {}\n");
        root.write("src/after.rs", "pub fn fixture() {}\n");
        root.write("src/lib.rs", lib);
        root.write("src/helpers.rs", "pub fn fixture() {}\n");
        root.write("src/gated.rs", "//! docs\n#![cfg(test)]\nfn t() {}\n");
        root.write("src/open.rs", "#![cfg(not(test))]\nfn t() {}\n");
        let before = crate::tools::test_code::parses();
        let out = run(
            &root.0,
            json!({"operation":"files","path":".","testCode":"only","entryType":"file","view":"paths"}),
        )
        .expect("listing");
        let mut listed = paths(&out)
            .iter()
            .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
            .collect::<Vec<_>>();
        listed.sort();
        assert_eq!(
            listed,
            ["docced.rs", "gated.rs", "helpers.rs", "noted.rs"],
            "{out}"
        );
        crate::tools::test_code::parses() - before
    };
    assert_eq!(parses(2), parses(16), "parses grew with production modules");
}
