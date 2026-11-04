use super::alternates::listed_passes;
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::empty_walk::with_probe_budget;
use crate::tools::local_search::executor::{execute_local_search, scan_query};
use crate::tools::local_search::types::LocalSearchResult;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

mod differential;

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    fs::write(path, text).expect("fixture");
}

fn query(root: &Path, fields: Value) -> LocalSearchQuery {
    let mut row = json!({"path": root});
    row.as_object_mut()
        .expect("row")
        .extend(fields.as_object().expect("fields").clone());
    serde_json::from_value(row.clone()).unwrap_or_else(|error| panic!("{row}: {error}"))
}

/// A fresh temporary tree under its real path.
fn tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real path");
    (dir, root)
}

/// One whole localSearch call, and the probe walks and listed passes it ran.
fn searched(root: &Path, fields: Value) -> (LocalSearchResult, usize, usize) {
    let paths = crate::tools::test_support::workspace_policy(root);
    let (walks, passes) = (probe_walks(), listed_passes());
    let row = execute_local_search(
        &query(root, fields),
        &paths,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    )
    .expect("search");
    (row, probe_walks() - walks, listed_passes() - passes)
}

fn hints(row: &LocalSearchResult) -> String {
    row.hints.join(" | ")
}

/// A walk whose ledger names nothing runs no relaxed walk; one thing named
/// runs it, and it still finds what the skip hid.
#[test]
fn a_walk_that_skipped_nothing_runs_no_relaxed_walk() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "nothing\n");
    let (row, walks, _) = searched(&root, json!({"matchString": ["needle"]}));
    assert_eq!(row.status, crate::tools::local_search::SearchStatus::Empty);
    assert_eq!(walks, 0, "{row:?}");
    assert!(row.hints.is_empty() && row.next.is_none(), "{row:?}");

    write(&root, ".dot.txt", "needle\n");
    let (row, walks, _) = searched(&root, json!({"matchString": ["needle"]}));
    assert_eq!(walks, 1);
    assert!(hints(&row).contains("behind skipped paths"), "{row:?}");
}

/// Withheld entries are named too: the walk keeps its check of them.
#[cfg(unix)]
#[test]
fn a_withheld_entry_keeps_the_check() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "nothing\n");
    std::os::unix::fs::symlink(root.join("a.txt"), root.join("link.txt")).expect("link");
    let (_, walks, _) = searched(&root, json!({"matchString": ["needle"]}));
    assert_eq!(walks, 1);
}

/// A CamelCase dotted miss: both readings of it from one pass over the first
/// walk's files, no walk of their own, each hint counting its own files.
#[test]
fn both_readings_of_a_miss_come_from_one_pass_over_the_walked_files() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "foo.bar\n");
    write(&root, "b.txt", "FooXBar\n");
    write(&root, "c.txt", "FooYBar\n");
    write(&root, "d.txt", "nothing\n");
    let (row, walks, passes) = searched(&root, json!({"matchString": ["Foo.Bar"]}));
    assert_eq!((walks, passes), (0, 1), "{row:?}");
    let shown = hints(&row);
    assert!(
        shown.contains("but 1 file(s) match with case ignored"),
        "{shown}"
    );
    assert!(
        shown.contains("but 2 file(s) match this read as a regex"),
        "{shown}"
    );
    assert!(
        row.next
            .as_ref()
            .is_some_and(|next| next.get("asRegex").is_some())
    );
}

#[test]
fn one_reading_alone_takes_its_own_pass_and_no_reading_takes_none() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "zzqx_nomatch\nzzqxYnomatch\n");
    let (row, walks, passes) = searched(&root, json!({"matchString": ["ZZQX_NOMATCH"]}));
    assert_eq!((walks, passes), (0, 1));
    assert!(hints(&row).contains("but 1 file(s) match with case ignored"));
    let (row, walks, passes) = searched(&root, json!({"matchString": ["zzqx.nomatch"]}));
    assert_eq!((walks, passes), (0, 1));
    assert!(hints(&row).contains("but 1 file(s) match this read as a regex"));
    let (row, walks, passes) = searched(&root, json!({"matchString": ["absent"]}));
    assert_eq!((walks, passes), (0, 0));
    assert!(row.hints.is_empty(), "{row:?}");
}

/// A record that cannot serve, or a shape the pass does not answer, walks as
/// before, and says the same.
#[test]
fn what_the_pass_cannot_answer_walks_and_says_the_same() {
    let (_dir, root) = tree();
    write(&root, "a.rs", "fn foo() {}\nmod tests;\n");
    write(&root, "b.rs", "fn foo() {}\n");
    let expect = "but 1 file(s) match with case ignored";
    let filtered = json!({"matchString": ["FOO"], "fileHas": ["tests"]});
    let (row, walks, passes) = searched(&root, filtered);
    assert_eq!((walks, passes), (1, 0));
    assert!(hints(&row).contains(expect), "{row:?}");
    let pcre2 = json!({"matchString": ["FOO\\(\\)(?= \\{)"], "regexEngine": "pcre2"});
    let (row, walks, passes) = searched(&root, pcre2);
    assert_eq!((walks, passes), (1, 0));
    assert!(
        hints(&row).contains("but 2 file(s) match with case ignored"),
        "{row:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_untracked_walk_record_walks_again() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "foo\n");
    write(&root, "rules", "*.log\n");
    // The linked ignore file also names a skipped entry; the walk stays whole.
    std::os::unix::fs::symlink(root.join("rules"), root.join(".ignore")).expect("link");
    let (row, walks, passes) = searched(&root, json!({"matchString": ["FOO"]}));
    assert_eq!(passes, 0);
    assert!(walks >= 1);
    assert!(
        hints(&row).contains("but 1 file(s) match with case ignored"),
        "{row:?}"
    );
}

/// A probe counts what the path policy admits and shows no content.
#[test]
fn a_probe_counts_admitted_files_and_shows_nothing_of_them() {
    let (_dir, root) = tree();
    let secret = "needle ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    write(&root, "a.txt", &format!("{secret}\n"));
    write(&root, "id_rsa", &format!("{secret}\n"));
    write(&root, ".env", &format!("{secret}\n"));
    let (row, _, _) = searched(&root, json!({"matchString": ["NEEDLE"]}));
    let shown = hints(&row);
    assert!(
        shown.contains("but 1 file(s) match with case ignored"),
        "{shown}"
    );
    let body = serde_json::to_string(&row.to_row().expect("row")).expect("json");
    assert!(!body.contains("ghp_0123"), "{body}");
}

/// Sleeps past the probe budget on every check after the first few, so a
/// probe keeps what those found and is cut before the rest.
struct Stalling {
    free: std::sync::atomic::AtomicIsize,
    stall: Duration,
}

impl CancellationCheck for Stalling {
    fn check(&self) -> Result<(), String> {
        if self.free.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) <= 0 {
            std::thread::sleep(self.stall);
        }
        Ok(())
    }
}

/// A cut probe's count is a floor and says so, by the pass and by the walk.
#[test]
fn a_cut_probe_says_at_least() {
    let (_dir, root) = tree();
    let many = octocode_engine::cpu::host_cores() * 4 + 8;
    for index in 0..many {
        write(&root, &format!("f{index:04}.txt"), "foo.bar\nFOOXBAR\n");
    }
    let paths = crate::tools::test_support::workspace_policy(&root);
    let written = query(&root, json!({"matchString": ["FOO.BAR"]}));
    let scan = scan_query(&written, &paths, &NeverCancel, (None, None)).expect("scan");
    assert!(scan.parsed.files.is_empty(), "must miss");
    let (mut row, _, _) = searched(&root, json!({"matchString": ["FOO.BAR"]}));
    let stalling = || Stalling {
        free: std::sync::atomic::AtomicIsize::new(4),
        stall: Duration::from_millis(60),
    };
    let budget = Duration::from_millis(30);
    for alternates in [
        Alternates::new(&written, &scan),
        Alternates::unlisted(&written),
    ] {
        let hint = with_probe_budget(budget, || {
            alternates.case_hint(&written, &paths, &scan.search_options, false, &stalling())
        })
        .expect("a floor to report");
        assert!(hint.contains("but at least "), "{hint}");
        row.hints.clear();
        row.next = None;
        with_probe_budget(budget, || {
            alternates.offer_regex_reading(&paths, &mut row, &stalling());
        });
        assert!(hints(&row).contains("but at least "), "{row:?}");
    }
}

/// Past its budget with nothing found, a probe has nothing to claim.
#[test]
fn a_probe_cut_before_any_find_stays_silent() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "foo\n");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let written = query(&root, json!({"matchString": ["FOO"]}));
    let scan = scan_query(&written, &paths, &NeverCancel, (None, None)).expect("scan");
    let alternates = Alternates::new(&written, &scan);
    let hint = with_probe_budget(Duration::ZERO, || {
        std::thread::sleep(Duration::from_millis(1));
        alternates.case_hint(&written, &paths, &scan.search_options, false, &NeverCancel)
    });
    assert_eq!(hint, None);
}

/// Every file holding the term is why `filesWithout` lists none: their case is
/// not what kept them off the page, so no case reading is offered.
#[test]
fn a_files_without_page_has_no_case_hint() {
    let (_dir, root) = tree();
    write(&root, "a.txt", "XYZ\n");
    write(&root, "b.txt", "XYZ\n");
    let (row, _, _) = searched(&root, json!({"matchString": ["XYZ"], "view": "filesWithout"}));
    assert_eq!(row.status, crate::tools::local_search::types::SearchStatus::Empty, "{row:?}");
    assert!(!hints(&row).contains("case"), "{row:?}");
}
