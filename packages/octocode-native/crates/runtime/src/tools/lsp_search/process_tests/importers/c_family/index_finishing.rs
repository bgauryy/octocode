//! A walk under way when clangd's background index finishes.
use super::*;

const ITS_INDEX: [(&str, &str); 1] = [(".cache/clangd/index/lib.c.0123ABCD.idx", "x")];

/// The index finishes while a walk is under way. The walk's own lead keeps
/// working: it finishes on the same list, each file once, and stays partial,
/// because text chose its files.
#[test]
fn a_window_lead_survives_the_index_finishing() {
    let names = (0..(crate::tools::lsp_search::importers::MAX_CANDIDATE_FILES * 2 + 5))
        .map(|index| format!("c{index:03}.c"))
        .collect::<Vec<_>>();
    let extra = names
        .iter()
        .map(|name| (name.as_str(), "int caller_fn(void) { return helper(1); }\n"))
        .collect::<Vec<_>>();
    for operation in ["callers", "references"] {
        let pages = walk_of(&Walk {
            name: "clangd",
            args: &["--background-index"],
            operation,
            extra: &extra,
            after_first_page: &ITS_INDEX,
            limit: None,
            lead: "nextImporterWindow",
        });
        assert!(pages.len() >= 3, "{operation}: {:?}", pages.last());
        let listed = pages.iter().flat_map(files_of).collect::<Vec<_>>();
        for name in &names {
            let times = listed.iter().filter(|file| file.ends_with(name)).count();
            assert_eq!(times, 1, "{operation}: {name} {times} time(s): {listed:?}");
        }
        for page in &pages {
            assert_ne!(page["status"], "error", "{operation}: {page}");
            assert_eq!(page["isPartial"], true, "{operation}: {page}");
        }
        let last = pages.last().expect("a page");
        assert_eq!(
            last["coverage"]["importerScan"], "complete",
            "{operation}: {last}"
        );
    }
}

/// A location page of the first window, once the index has finished, cannot go
/// on: the index's answer is another list. It says so and restarts, which answers
/// from the index.
#[test]
fn a_location_page_across_the_index_finishing_restarts() {
    let extra = [
        ("c000.c", "int caller_fn(void) { return helper(1); }\n"),
        ("c001.c", "int caller_fn(void) { return helper(1); }\n"),
    ];
    let pages = walk_of(&Walk {
        name: "clangd",
        args: &["--background-index"],
        operation: "references",
        extra: &extra,
        after_first_page: &ITS_INDEX,
        limit: Some(1),
        lead: "nextPage",
    });
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert_eq!(pages[1]["errorCode"], "staleSnapshot", "{}", pages[1]);
    let restart = &crate::tools::test_support::checks::related_call(&pages[1], "lspSearch", |_| {
        true
    })["input"]["queries"][0];
    assert!(restart.get("snapshot").is_none(), "{restart}");
    assert!(restart.get("page").is_none(), "{restart}");
}
