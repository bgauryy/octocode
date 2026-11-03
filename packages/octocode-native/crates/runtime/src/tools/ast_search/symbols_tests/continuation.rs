//! A directory outline's continuation pages: the pages together are the
//! whole pass exactly once, a page after the first parses nothing, and a
//! changed directory restarts instead of serving the stored set.

use super::super::paging_tests::walk;
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
