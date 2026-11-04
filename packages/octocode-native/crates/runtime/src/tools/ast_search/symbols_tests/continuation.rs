//! An outline's continuation pages: the pages together are the whole pass
//! exactly once, a page after the first parses nothing, and a changed file or
//! directory restarts instead of serving the stored set.

use super::super::paging_tests::{numbered_functions, symbol_rows, walk, walk_within};
use super::scans::write_settled;
use super::*;

const FILES: usize = 12;

/// `FILES` settled files of two top-level declarations each.
fn settled_files(root: &std::path::Path) {
    for index in 0..FILES {
        write_settled(
            &root.join(format!("file{index:02}.ts")),
            &format!("export function helper{index}() {{}}\nexport class Widget{index} {{}}\n"),
            index as u64,
        );
    }
    // The fixture's writes count as settled, as if the test had waited out
    // the window a stored set needs.
    octocode_engine::stamp::settle_written_so_far();
}

/// Every parse of a file under `root` so far.
fn parses(root: &std::path::Path) -> usize {
    let root = root.canonicalize().expect("root");
    super::super::symbols::PARSED_FILES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|path| std::path::Path::new(path).starts_with(&root))
        .count()
}

/// The symbol names of a page, in order.
fn names(page: &serde_json::Value) -> Vec<String> {
    page["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["symbols"].as_array().into_iter().flatten())
        .filter_map(|symbol| symbol["symbolName"].as_str().map(str::to_owned))
        .collect()
}

/// The query of page 2 of the result whose first page is `first`, with
/// `change` applied: a continuation the way a client builds it.
fn second_page(
    root: &std::path::Path,
    first: &serde_json::Value,
    change: serde_json::Value,
) -> serde_json::Value {
    let mut row = json!({"operation":"symbols","path":root.to_string_lossy(),"limit":3,"page":2,"snapshot":first["snapshot"]});
    row.as_object_mut()
        .expect("object")
        .extend(change.as_object().expect("object").clone());
    run_lead(root, row)
}

/// Walking every page reconstructs the whole-pass symbols exactly once, and
/// only the first page parses: the files are outlined once for the result.
#[test]
fn the_pages_of_a_directory_outline_are_the_whole_pass_and_parse_once() {
    let root = Fixture::new();
    settled_files(&root.0);
    let whole = names(&empty_outline(&root.0, json!({})));
    assert_eq!(whole.len(), FILES * 2, "{whole:?}");

    let before = parses(&root.0);
    let pages = walk(
        &root.0,
        json!({"operation":"symbols","path":root.0,"limit":3}),
        names,
    );
    assert!(pages.len() > 2, "the result spans several pages");
    let walked = pages.iter().flat_map(names).collect::<Vec<_>>();
    assert_eq!(
        walked, whole,
        "every symbol once, in the whole pass's order"
    );
    assert_eq!(
        parses(&root.0) - before,
        FILES,
        "the first page parses each file once and no later page parses"
    );
}

/// An edit, a new file or a removed one after page 1 leaves the stored set
/// unserved: the page outlines afresh and the old snapshot is stale.
#[test]
fn a_changed_directory_restarts_the_continuation() {
    for change in ["edit", "add", "remove"] {
        let root = Fixture::new();
        settled_files(&root.0);
        let first = empty_outline(&root.0, json!({"limit":3}));
        assert!(first["snapshot"].is_string(), "{first}");
        match change {
            "edit" => {
                std::fs::write(
                    root.0.join("file05.ts"),
                    "export function renamed() {}\nexport class Widget5 {}\n",
                )
                .expect("edit");
            }
            "add" => {
                std::fs::write(root.0.join("added.ts"), "export function added() {}\n")
                    .expect("add");
            }
            _ => std::fs::remove_file(root.0.join("file05.ts")).expect("remove"),
        }
        let stale = second_page(&root.0, &first, json!({}));
        assert_eq!(stale["errorCode"], "staleSnapshot", "{change}: {stale}");
    }
}

/// An unchanged directory serves page 2 from the stored set.
#[test]
fn an_unchanged_directory_serves_the_next_page_without_parsing() {
    let root = Fixture::new();
    settled_files(&root.0);
    let first = empty_outline(&root.0, json!({"limit":3}));
    let parsed = parses(&root.0);
    let second = second_page(&root.0, &first, json!({}));
    assert!(second.get("errorCode").is_none(), "{second}");
    assert_eq!(names(&second).len(), 3, "{second}");
    assert_eq!(parses(&root.0), parsed, "page 2 parsed a file");
}

/// A continuation whose query differs from the first page's is not served
/// the stored set of that snapshot.
#[test]
fn a_continuation_with_another_filter_is_not_served_the_stored_set() {
    let root = Fixture::new();
    settled_files(&root.0);
    let first = empty_outline(&root.0, json!({"limit":3}));
    let answer = second_page(&root.0, &first, json!({"symbolKinds":["class"]}));
    assert_eq!(answer["errorCode"], "staleSnapshot", "{answer}");
}

/// A settled file of `count` top-level declarations.
fn settled_file(root: &std::path::Path, count: usize) -> std::path::PathBuf {
    let file = root.join("big.ts");
    write_settled(&file, &numbered_functions(count), 0);
    octocode_engine::stamp::settle_written_so_far();
    file
}

/// Walking a file outline's pages parses it once: only the first page reads
/// the file, and the rows are every declaration once.
#[test]
fn the_pages_of_a_file_outline_parse_once() {
    let root = Fixture::new();
    let file = settled_file(&root.0, 1_500);
    let before = parses(&root.0);
    let pages = walk(
        &root.0,
        json!({"operation":"symbols","path":file}),
        symbol_rows,
    );
    assert!(pages.len() > 2, "the result spans several pages");
    assert_eq!(pages.iter().flat_map(symbol_rows).count(), 1_500);
    assert_eq!(parses(&root.0) - before, 1, "only the first page parses");
}

/// A file edited after a page was served from the stored set is not served
/// from it again: an edit that changes the declarations answers
/// `staleSnapshot`, even one that restores the size and modified time, and an
/// edit that changes none reads the file again and serves the same result.
#[test]
fn an_edited_file_restarts_its_continuation() {
    enum Edit {
        Renamed,
        RenamedKeepingSizeAndTime,
        Appended,
        Comment,
    }
    for edit in [
        Edit::Renamed,
        Edit::RenamedKeepingSizeAndTime,
        Edit::Appended,
        Edit::Comment,
    ] {
        let root = Fixture::new();
        let file = settled_file(&root.0, 30);
        let page = |first: &serde_json::Value, page: u64| {
            run_lead(
                &root.0,
                json!({"operation":"symbols","path":file,"limit":3,"page":page,"snapshot":first["snapshot"]}),
            )
        };
        let first = run_lead(
            &root.0,
            json!({"operation":"symbols","path":file,"limit":3}),
        );
        let parsed = parses(&root.0);
        let second = page(&first, 2);
        assert!(second.get("errorCode").is_none(), "{second}");
        assert_eq!(parses(&root.0), parsed, "page 2 parsed the file");

        let source = std::fs::read_to_string(&file).expect("source");
        let modified = std::fs::metadata(&file)
            .expect("metadata")
            .modified()
            .expect("modified");
        let changes_rows = match edit {
            Edit::Renamed => {
                std::fs::write(&file, source.replace("declaredName20(", "renamedName(")).is_ok()
            }
            Edit::RenamedKeepingSizeAndTime => {
                std::fs::write(&file, source.replace("declaredName20(", "declaredNameXY("))
                    .expect("edit");
                std::fs::File::options()
                    .write(true)
                    .open(&file)
                    .and_then(|edited| edited.set_modified(modified))
                    .is_ok()
            }
            Edit::Appended => {
                std::fs::write(&file, format!("{source}export function added() {{}}\n")).is_ok()
            }
            Edit::Comment => {
                std::fs::write(&file, format!("{source}// a comment\n")).expect("edit");
                false
            }
        };
        let third = page(&first, 3);
        if changes_rows {
            assert_eq!(third["errorCode"], "staleSnapshot", "{third}");
        } else {
            assert!(third.get("errorCode").is_none(), "{third}");
            assert_eq!(parses(&root.0), parsed + 1, "the edit read the file again");
        }
    }
}

/// A `testCode` scope reads more than the file: its parent module can gate it
/// under test, and an edit there changes the answer.
#[test]
fn a_test_scoped_file_outline_follows_its_parent_module() {
    let root = Fixture::new();
    let parent = root.0.join("lib.rs");
    write_settled(&parent, "pub mod big;\n", 0);
    let file = root.0.join("big.rs");
    let source = (0..12).fold(String::new(), |source, index| {
        source + &format!("pub fn declared{index}() {{}}\n")
    });
    write_settled(&file, &source, 0);
    octocode_engine::stamp::settle_written_so_far();
    let row = json!({"operation":"symbols","path":file,"testCode":"exclude","limit":3});
    let first = run_lead(&root.0, row.clone());
    assert!(first["snapshot"].is_string(), "{first}");
    let mut next = row;
    next["page"] = json!(2);
    next["snapshot"] = first["snapshot"].clone();
    let second = run_lead(&root.0, next.clone());
    assert!(second.get("errorCode").is_none(), "{second}");
    std::fs::write(&parent, "#[cfg(test)]\npub mod big;\n").expect("gate");
    let third = run_lead(&root.0, next);
    assert_eq!(third["errorCode"], "staleSnapshot", "{third}");
}

const OPEN: &str = "pub mod sub;\n";
const GATED: &str = "#[cfg(test)]\npub mod sub;\n";

/// `src/sub`, a module of `src/lib.rs` (outside the outline's scope) that says
/// `declares` it, with its settled submodules; the path of `lib.rs`.
fn module_beside_its_parent(root: &std::path::Path, declares: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(root.join("src/sub")).expect("fixture dirs");
    let parent = root.join("src/lib.rs");
    write_settled(&parent, declares, 0);
    write_settled(&root.join("src/sub/mod.rs"), "pub mod a;\npub mod b;\n", 0);
    for module in ["a", "b"] {
        let source = (0..4).fold(String::new(), |source, index| {
            source + &format!("pub fn {module}{index}() {{}}\n")
        });
        write_settled(&root.join(format!("src/sub/{module}.rs")), &source, 0);
    }
    octocode_engine::stamp::settle_written_so_far();
    parent
}

/// A directory outline under a `testCode` scope follows a parent module
/// outside its scope: moving the module across the line leaves the pages
/// stale, on either side (`only` reads the same files whatever the parent says).
#[test]
fn a_test_scoped_directory_outline_follows_a_parent_outside_its_scope() {
    for (test_code, before, after) in [("exclude", OPEN, GATED), ("only", GATED, OPEN)] {
        let root = Fixture::new();
        let parent = module_beside_its_parent(&root.0, before);
        let row = json!({"operation":"symbols","path":root.0.join("src/sub"),"testCode":test_code,"limit":3});
        let first = run_lead(&root.0, row.clone());
        assert!(first["snapshot"].is_string(), "{test_code}: {first}");
        let mut next = row;
        next["page"] = json!(2);
        next["snapshot"] = first["snapshot"].clone();
        let second = run_lead(&root.0, next.clone());
        assert!(second.get("errorCode").is_none(), "{test_code}: {second}");
        std::fs::write(&parent, after).expect("edit");
        let third = run_lead(&root.0, next);
        assert_eq!(third["errorCode"], "staleSnapshot", "{test_code}: {third}");
    }
}

/// Without a filtering scope nothing depends on a parent: every later page
/// is served from the stored set, parsing nothing, whatever a parent says.
#[test]
fn an_outline_with_no_test_code_filter_keeps_its_stored_set() {
    for test_code in [None, Some("include")] {
        let root = Fixture::new();
        let parent = module_beside_its_parent(&root.0, OPEN);
        let mut row = json!({"operation":"symbols","path":root.0.join("src/sub"),"limit":3});
        if let Some(test_code) = test_code {
            row["testCode"] = json!(test_code);
        }
        let first = run_lead(&root.0, row.clone());
        let mut next = row;
        next["page"] = json!(2);
        next["snapshot"] = first["snapshot"].clone();
        let parsed = parses(&root.0);
        std::fs::write(&parent, GATED).expect("gate");
        let second = run_lead(&root.0, next);
        assert!(second.get("errorCode").is_none(), "{second}");
        assert_eq!(parses(&root.0), parsed, "served from the stored set");
    }
}

/// Pages cut from the stored set are the pages a fresh outline delivers, at
/// any response window: the row sizes the set keeps are the delivered ones.
#[test]
fn stored_pages_equal_fresh_pages_at_every_window() {
    for window in [2_048, 16_384] {
        for directory in [false, true] {
            let root = Fixture::new();
            let (scope, files) = if directory {
                for index in 0..400 {
                    root.write(
                        &format!("some-long-package-name/src/file_{index:03}.ts"),
                        format!("export function a{index}() {{}}\nexport class B{index} {{}}\n"),
                    );
                }
                (root.0.join("some-long-package-name"), 400)
            } else {
                (root.0.join("big.ts"), 1)
            };
            if !directory {
                root.write("big.ts", numbered_functions(400));
            }
            let row = json!({"operation":"symbols","path":scope});
            // This thread's clock counts nothing as settled: every file was
            // just written, so no page is stored and each one reads afresh.
            let fresh = std::thread::scope(|threads| {
                let (root, row) = (&root.0, row.clone());
                threads
                    .spawn(move || walk_within(root, row, symbol_rows, window))
                    .join()
                    .expect("fresh walk")
            });
            octocode_engine::stamp::settle_written_so_far();
            let before = parses(&root.0);
            let stored = walk_within(&root.0, row, symbol_rows, window);
            assert!(stored.len() > 1, "the result spans several pages");
            assert_eq!(parses(&root.0) - before, files, "only page 1 parses");
            assert_eq!(stored, fresh, "window {window}, directory {directory}");
            for (index, page) in stored.iter().enumerate() {
                let chars = octocode_engine::json_size::utf16_units(page).unwrap_or(0);
                assert!(chars <= window, "page {index} is {chars} chars");
            }
        }
    }
}
