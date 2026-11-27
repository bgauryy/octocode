//! A scan read across an edit, window by window (`next.expandScan`): every
//! surviving file exactly once, or `staleSnapshot` with its restart. `scanOffset`
//! is a count of candidates, so a window pins the candidates before it.
use crate::ast_search::test_rows::run;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::test_support::Fixture;
use serde_json::{Value, json};
use std::time::Duration;

const SOURCE: &str = "pub fn source() {}\n";

fn fixture() -> Fixture {
    let root = Fixture::new();
    for name in ["b", "c", "d", "e", "f", "g", "h", "i"] {
        root.write(&format!("{name}.rs"), SOURCE);
    }
    root
}

/// The queries whose directory scans resume by `scanOffset`, in walk order
/// and in a ranked order (whose cut windows list nothing until the last).
const QUERIES: usize = 4;

fn queries(root: &Fixture) -> [Value; QUERIES] {
    [
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"}}),
        json!({"operation":"match","path":root.0,"parser":"rust","view":"files","rule":{"pattern":"pub fn source() {}"}}),
        json!({"operation":"symbols","path":root.0}),
        json!({"operation":"match","path":root.0,"parser":"rust","sort":{"by":"matchCount","order":"desc"},"rule":{"pattern":"pub fn source() {}"}}),
    ]
}

/// The files a window lists, by name.
fn files(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| {
            let path = file["path"].as_str().expect("file path");
            path.rsplit('/').next().expect("name").to_owned()
        })
        .collect()
}

fn is_stale(out: &Value) -> bool {
    out["errorCode"] == json!(ErrorCode::StaleSnapshot)
}

enum Ended {
    Files(Vec<String>),
    Stale,
}

/// Reads the windows from `out` on, `edit` applied after `edits_after` of them.
fn walk(root: &Fixture, query: Value, edits_after: usize, edit: impl FnOnce()) -> Ended {
    let mut listed = Vec::new();
    let mut edit = Some(edit);
    let mut next = query;
    for window in 0..50 {
        let out = run(&root.0, next.clone()).expect("window");
        if is_stale(&out) {
            let restart = &out["related"][0]["input"]["queries"][0];
            assert!(restart.is_object(), "{out}");
            for cursor in ["scanOffset", "snapshot", "page"] {
                assert!(restart.get(cursor).is_none(), "{cursor}: {out}");
            }
            return Ended::Stale;
        }
        listed.extend(files(&out));
        if window + 1 == edits_after
            && let Some(edit) = edit.take()
        {
            std::thread::sleep(Duration::from_millis(20));
            edit();
        }
        match out["next"]
            .get("expandScan")
            .or_else(|| out["next"].get("nextPage"))
        {
            Some(lead) => next = lead["input"]["queries"][0].clone(),
            None => return Ended::Files(listed),
        }
    }
    panic!("the scan never finishes: {next}");
}

fn every_query(edit: impl Fn(&Fixture), expected: impl Fn(&[String]) -> Vec<String>, case: &str) {
    for at in 0..QUERIES {
        let root = fixture();
        let query = queries(&root)[at].clone();
        let _window = super::test_window(2);
        match walk(&root, query.clone(), 1, || edit(&root)) {
            Ended::Stale => {}
            Ended::Files(mut listed) => {
                listed.sort();
                let all = ["b", "c", "d", "e", "f", "g", "h", "i"].map(|name| format!("{name}.rs"));
                assert_eq!(listed, expected(&all), "{case}: {query}");
            }
        }
    }
}

#[test]
fn removing_a_file_an_earlier_window_listed_loses_no_file() {
    every_query(
        |root| std::fs::remove_file(root.0.join("b.rs")).expect("remove"),
        // The removed file was listed before it went.
        <[String]>::to_vec,
        "removed",
    );
}

/// A count would land one file early and list one file twice.
#[test]
fn inserting_a_file_before_the_cut_repeats_no_file() {
    every_query(
        |root| {
            root.write("a.rs", SOURCE);
        },
        |all| {
            let mut all = all.to_vec();
            all.push("a.rs".to_owned());
            all.sort();
            all
        },
        "inserted",
    );
}

fn restarts(edit: impl Fn(&Fixture), edits_after: usize, case: &str) {
    for at in 0..QUERIES {
        let root = fixture();
        let query = queries(&root)[at].clone();
        let _window = super::test_window(2);
        assert!(
            matches!(
                walk(&root, query.clone(), edits_after, || edit(&root)),
                Ended::Stale
            ),
            "{case}: {query}"
        );
    }
}

#[test]
fn every_edit_before_the_cut_restarts_the_scan() {
    restarts(
        |root| std::fs::remove_file(root.0.join("c.rs")).expect("remove"),
        1,
        "removed",
    );
    restarts(
        |root| {
            root.write("a.rs", SOURCE);
        },
        1,
        "inserted",
    );
    restarts(
        |root| {
            root.write("b.rs", "pub fn source() {}\npub fn other() {}\n");
        },
        1,
        "edited",
    );
}

/// The pin holds across every window, not only the next one.
#[test]
fn an_edit_of_the_first_window_restarts_a_later_one() {
    restarts(
        |root| std::fs::remove_file(root.0.join("b.rs")).expect("remove"),
        2,
        "removed",
    );
}

/// What the walk has not reached it has not seen: it lists what is there when
/// it gets there.
#[test]
fn a_file_added_past_the_cut_is_listed_once_with_no_restart() {
    for at in 0..QUERIES {
        let root = fixture();
        let query = queries(&root)[at].clone();
        let _window = super::test_window(2);
        let Ended::Files(mut listed) = walk(&root, query.clone(), 1, || {
            root.write("z.rs", SOURCE);
        }) else {
            panic!("the first window's files did not change: {query}");
        };
        listed.sort();
        let mut expected = ["b", "c", "d", "e", "f", "g", "h", "i", "z"]
            .map(|name| format!("{name}.rs"))
            .to_vec();
        expected.sort();
        assert_eq!(listed, expected, "{query}");
    }
}

#[test]
fn an_unchanged_scan_lists_every_file_once() {
    for at in 0..QUERIES {
        let root = fixture();
        let query = queries(&root)[at].clone();
        let _window = super::test_window(3);
        let Ended::Files(listed) = walk(&root, query.clone(), 0, || {}) else {
            panic!("nothing changed: {query}");
        };
        assert_eq!(
            listed,
            ["b", "c", "d", "e", "f", "g", "h", "i"].map(|name| format!("{name}.rs")),
            "{query}"
        );
    }
}

/// A later page of a later window answers for the windows before it too: its
/// rows' file did not change, the rows of the first window did.
#[test]
fn an_edit_of_an_earlier_window_restarts_a_later_page() {
    let root = Fixture::new();
    // `c.rs` holds two matches: the second window (its page 1 needs it)
    // has two pages.
    root.write("b.rs", SOURCE);
    root.write("c.rs", format!("{SOURCE}{SOURCE}"));
    root.write("d.rs", SOURCE);
    let row = json!({"operation":"match","path":root.0,"parser":"rust","limit":1,"rule":{"pattern":"pub fn source() {}"}});
    let _window = super::test_window(2);
    // First window: `b.rs`, its one page; the next page is the next window's.
    let first = run(&root.0, row).expect("page 1");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next.get("scanOffset").is_some(), "{first}");
    // Second window, first page.
    let second = run(&root.0, next).expect("window 2 page 1");
    assert!(!is_stale(&second), "{second}");
    let next = second["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next.is_object(), "{second}");
    std::thread::sleep(Duration::from_millis(20));
    root.write("b.rs", "pub fn source() {}\npub fn other() {}\n");
    let third = run(&root.0, next).expect("window 2 page 2");
    assert!(is_stale(&third), "{third}");
}
