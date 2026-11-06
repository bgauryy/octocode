use super::alternates::listed_passes;
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::empty_walk::{CUT_AT_ONCE, ProbeLimits, with_probe_limits};
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

/// Sleeps `stall` on every check after the first `free`: load that a
/// probe's finding must not depend on.
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

/// The skipped-path sentence of an empty search of `written`, under `cancel`.
fn skipped_hint(
    paths: &PathPolicy,
    written: &LocalSearchQuery,
    cancel: &impl CancellationCheck,
) -> String {
    let row = execute_local_search(written, paths, &ContentSecurity::new(), cancel, None, None)
        .expect("search");
    row.hints
        .iter()
        .find(|hint| hint.contains("behind skipped paths"))
        .cloned()
        .unwrap_or_else(|| panic!("no skipped-path finding: {row:?}"))
}

/// What the skipped paths hold is counted in work, never in time: the same
/// tree gives the same finding however slowly every check answers, whether
/// the probe finishes or its work cuts it.
#[test]
fn the_skipped_path_count_is_the_same_under_load() {
    let (_dir, root) = tree();
    write(&root, "src/a.txt", "nothing\n");
    let behind = 200;
    for index in 0..behind {
        write(
            &root,
            &format!("node_modules/p{index:03}/i.txt"),
            "needle\n",
        );
    }
    let paths = crate::tools::test_support::workspace_policy(&root);
    let written = query(&root, json!({"matchString": ["needle"]}));
    let loaded = || Stalling {
        free: std::sync::atomic::AtomicIsize::new(0),
        stall: Duration::from_millis(2),
    };
    let idle = skipped_hint(&paths, &written, &NeverCancel);
    assert!(
        idle.starts_with(&format!("{behind} files that match behind skipped paths")),
        "{idle}"
    );
    assert_eq!(skipped_hint(&paths, &written, &loaded()), idle);
    let cut = ProbeLimits {
        entries: 50,
        bytes: u64::MAX,
    };
    let (idle, under_load) = with_probe_limits(cut, || {
        (
            skipped_hint(&paths, &written, &NeverCancel),
            skipped_hint(&paths, &written, &loaded()),
        )
    });
    assert!(idle.starts_with("At least "), "{idle}");
    assert_eq!(under_load, idle);
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
    let budget = ProbeLimits {
        entries: 4,
        bytes: u64::MAX,
    };
    for alternates in [
        Alternates::new(&written, &scan),
        Alternates::unlisted(&written),
    ] {
        let hint = with_probe_limits(budget, || {
            alternates.case_hint(&written, &paths, &scan.search_options, false, &NeverCancel)
        })
        .expect("a floor to report");
        assert!(hint.contains("but at least "), "{hint}");
        row.hints.clear();
        row.next = None;
        with_probe_limits(budget, || {
            alternates.offer_regex_reading(&paths, &mut row, &NeverCancel);
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
    let hint = with_probe_limits(CUT_AT_ONCE, || {
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
    let (row, _, _) = searched(
        &root,
        json!({"matchString": ["XYZ"], "view": "filesWithout"}),
    );
    assert_eq!(
        row.status,
        crate::tools::local_search::types::SearchStatus::Empty,
        "{row:?}"
    );
    assert!(!hints(&row).contains("case"), "{row:?}");
}

/// The check of the skipped paths reads only what the empty walk did not: the
/// file it searched leaves the check's reach, while what the walk skipped stays
/// in it, so a match behind a skipped path is still named.
#[test]
fn the_skipped_check_reads_only_what_the_walk_did_not() {
    let (_dir, root) = tree();
    std::fs::create_dir_all(root.join(".git")).expect("marker");
    std::fs::write(root.join(".gitignore"), "gen/\n").expect("rules");
    std::fs::create_dir_all(root.join("gen")).expect("dir");
    std::fs::write(root.join("seen.txt"), "needle\n").expect("seen");
    std::fs::write(root.join("gen/hidden.txt"), "needle\n").expect("hidden");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let options = crate::tools::local_search::executor::search_options(
        &query(&root, serde_json::json!({"matchString": ["needle"]})),
        &root,
    );
    let row = query(&root, serde_json::json!({"matchString": ["needle"]}));
    let searched = [root.join("seen.txt")];
    let lifted = probe_unsearched(&row, &paths, &options, Some(&searched), &NeverCancel)
        .unwrap_or_else(|()| panic!("probe"));
    let found = lifted.tagged.find(
        Walk {
            ignored: true,
            hidden: true,
            default_excludes: false,
        },
        None,
    );
    assert_eq!(found.count, 1, "gen/hidden.txt alone");
}
