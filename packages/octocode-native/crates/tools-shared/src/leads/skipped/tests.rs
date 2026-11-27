use super::*;
use octocode_test_support::json_fixture;

/// A lead is the query's search, not the caller's words about it: the
/// brief fields are the caller's to restate for the new call.
#[test]
fn a_lead_repeats_the_search_without_the_callers_brief() {
    let query = json_fixture(
        r#"{
            "path": "src",
            "matchString": ["needle"],
            "mainGoal": "the task",
            "reasoning": "why this query"
        }"#,
    );
    let lead = lead(ToolId::LocalSearch, &query, &[Cause::Hidden]);
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["path"], "src", "{lead}");
    assert_eq!(row["includeHidden"], true, "{lead}");
    assert!(row.get("mainGoal").is_none(), "{lead}");
    assert!(row.get("reasoning").is_none(), "{lead}");
}

/// One sentence shape whatever the tool: the row's `skippedPaths` were not
/// searched, and the one related call (in the call bundle) lifts exactly
/// their causes, each named by its flag, from the first page. The requested
/// walk was whole, so nothing is partial.
#[test]
fn every_tool_discloses_skipped_paths_in_one_shape() {
    assert_eq!(unsearched(&[]), None);
    let said = unsearched(&[Cause::Ignored, Cause::DefaultExcluded]).expect("said");
    assert_eq!(said.causes, [Cause::Ignored, Cause::DefaultExcluded]);
    assert!(said.text.contains("skippedPaths"), "{}", said.text);
    for cause in &said.causes {
        let (field, value) = cause.flag();
        assert!(
            said.text.contains(&format!("{field}:{value}")),
            "{}",
            said.text
        );
    }
    let (hidden, _) = Cause::Hidden.flag();
    assert!(!said.text.contains(hidden), "{}", said.text);
    let mut out = serde_json::json!({});
    attach(
        &mut out,
        &said,
        lead(
            ToolId::StructureSearch,
            &json_fixture(
                r#"{
                    "operation": "files",
                    "path": ".",
                    "page": 3,
                    "snapshot": "x"
                }"#,
            ),
            &said.causes,
        ),
    );
    assert!(
        out.get("partialReasons").is_none(),
        "the requested walk was whole"
    );
    // A row with evidence: the sentence limits what it proves absent (a
    // warning), and the call is one the row needs.
    assert_eq!(out["warnings"], serde_json::json!([said.text]));
    assert!(out.get("hints").is_none(), "{out}");
    assert_eq!(out["calls"]["includeSkipped"]["kind"], "related");
    assert_eq!(
        out["calls"]["includeSkipped"][octocode_runtime_base::response::result::REQUIRED_KEY],
        true
    );
    // An empty row: the same sentence is its recovery hint, and its call is
    // an ordinary lead.
    let mut empty = serde_json::json!({"status": "empty"});
    attach(
        &mut empty,
        &said,
        lead(
            ToolId::StructureSearch,
            &json_fixture(r#"{"operation": "files", "path": "."}"#),
            &said.causes,
        ),
    );
    assert_eq!(empty["hints"], serde_json::json!([said.text]));
    assert!(empty.get("warnings").is_none(), "{empty}");
    assert!(
        empty["calls"]["includeSkipped"]
            .get(octocode_runtime_base::response::result::REQUIRED_KEY)
            .is_none(),
        "{empty}"
    );
    let row = &out["calls"]["includeSkipped"]["input"]["queries"][0];
    assert_eq!(row["includeIgnored"], true, "{row}");
    assert_eq!(row["defaultExcludes"], false, "{row}");
    assert!(
        row.get("page").is_none() && row.get("snapshot").is_none(),
        "{row}"
    );
}

/// A walk's ledger is disclosed by its causes, the default excludes too for
/// an ignored entry they would also leave out.
#[test]
fn a_ledger_is_disclosed_by_its_counts() {
    let mut walked = octocode_runtime_base::policy::discovery::WalkSkips::default();
    assert_eq!(unsearched_ledger(&walked), None);
    walked.ignored.insert("target/".to_owned());
    walked.ignored_excluded = true;
    walked.hidden.insert(".notes/".to_owned());
    let said = unsearched_ledger(&walked).expect("said");
    assert_eq!(
        said.causes,
        [Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded]
    );
    assert_eq!(Some(&said), unsearched(&said.causes).as_ref());
}

/// Final round, item 4: a skip's disclosure quotes the walk's path and
/// reason (a rule's text, an error) as judged, so a credential written there
/// shows as the placeholder a read shows; the runtime's own words stay.
#[test]
fn a_disclosure_quotes_the_walks_text_as_judged() {
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let entry = |cause: SkippedPathCause, reason: Option<String>| SkippedPath {
        path: "logs".to_owned().try_into().expect("path"),
        cause,
        count: None,
        reason,
    };
    for cause in [SkippedPathCause::Unreadable, SkippedPathCause::Ignored] {
        let shown = disclosure(
            &entry(cause, Some(format!("rule `token={token}` in .gitignore"))),
            &format!("logs/{token}.txt"),
        );
        assert!(!shown.contains(&token), "{shown}");
        assert!(shown.contains("[REDACTED-"), "{shown}");
        let plain = disclosure(&entry(cause, Some("permission denied".into())), "logs");
        assert!(
            plain.contains("`logs`") && plain.contains("permission denied"),
            "{plain}"
        );
    }
}

/// A withheld entry is the directory holding what the policy left out, with
/// its count: entries of other directories are never merged
/// into a shared ancestor, with or without every path kept. Every count is
/// kept.
#[test]
fn withheld_entries_stay_one_counted_entry_per_holding_directory() {
    let withheld = |path: &str, reason: &str, count: usize| SkippedEntry {
        path: path.to_owned(),
        cause: SkippedPathCause::Withheld,
        reason: Some(reason.to_owned()),
        count,
    };
    let entries = [
        withheld("/w/ext/a", "credential file", 1),
        withheld("/w/ext/b/c", "credential file", 2),
        withheld("/w/ext/d", "symlink not followed", 1),
        withheld("/w/ext/a/e", "credential file", 1),
        // A holding directory reported again after another one joins its own.
        withheld("/w/ext/b/c", "credential file", 1),
    ];
    let total = |paths: &SkippedPaths, reason: &str| {
        paths
            .entries
            .iter()
            .filter(|entry| entry.reason.as_deref() == Some(reason))
            .map(|entry| entry.count.unwrap_or(1))
            .sum::<i64>()
    };
    let grouped = skipped_paths(entries.clone(), false, false)
        .expect("valid")
        .expect("entries");
    let credentials = grouped
        .entries
        .iter()
        .filter(|entry| entry.reason.as_deref() == Some("credential file"))
        .collect::<Vec<_>>();
    let mut held = credentials
        .iter()
        .map(|entry| (entry.path.as_str().to_owned(), entry.count))
        .collect::<Vec<_>>();
    held.sort();
    assert_eq!(
        held,
        [
            ("/w/ext/a".to_owned(), None),
            ("/w/ext/a/e".to_owned(), None),
            ("/w/ext/b/c".to_owned(), Some(3)),
        ],
        "{grouped:?}"
    );
    assert!(
        grouped.sampled != Some(true),
        "count marks a collapse; a finished walk is no sample: {grouped:?}"
    );
    let every = skipped_paths(entries.clone(), true, false)
        .expect("valid")
        .expect("entries");
    let distinct = entries
        .iter()
        .map(|entry| (&entry.path, &entry.reason))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(every.entries.len(), distinct.len(), "{every:?}");
    for reason in ["credential file", "symlink not followed"] {
        let written = entries
            .iter()
            .filter(|entry| entry.reason.as_deref() == Some(reason))
            .map(|entry| saturating_i64(entry.count))
            .sum::<i64>();
        assert_eq!(total(&grouped, reason), written, "{reason}: {grouped:?}");
        assert_eq!(total(&every, reason), written, "{reason}: {every:?}");
    }
}

/// `sampled` says the walk did not finish, whatever was collapsed: an
/// unfinished walk is sampled, a collapsed finished one is not, and two
/// disclosures appended are sampled when either walk was.
#[test]
fn sampled_follows_the_walk_not_the_collapse() {
    let entry = |path: &str, count: usize| SkippedEntry {
        path: path.to_owned(),
        cause: SkippedPathCause::Withheld,
        reason: Some("credential file".to_owned()),
        count,
    };
    let collapsed = |incomplete| {
        skipped_paths([entry("/w/a", 1), entry("/w/b", 2)], false, incomplete)
            .expect("valid")
            .expect("entries")
    };
    assert_eq!(collapsed(true).sampled, Some(true));
    assert_ne!(collapsed(false).sampled, Some(true));
    let both = appended(Some(collapsed(false)), Some(collapsed(true))).expect("both");
    assert_eq!(both.sampled, Some(true), "{both:?}");
    let finished = appended(Some(collapsed(false)), Some(collapsed(false))).expect("both");
    assert_ne!(finished.sampled, Some(true), "{finished:?}");
}

/// A path skipped both as ignored and as default-excluded is one entry,
/// `ignored-default-excluded`, where it was first named; every other path
/// keeps its own cause, and each path is listed once.
#[test]
fn a_path_skipped_for_both_lifted_causes_is_one_entry() {
    let entry = |path: &str, cause| SkippedEntry::path(path, cause);
    use SkippedPathCause::{DefaultExcluded, Hidden, Ignored, IgnoredDefaultExcluded};
    let shown = skipped_paths(
        [
            entry("/w/a/build", Ignored),
            entry("/w/.x", Hidden),
            entry("/w/node_modules", DefaultExcluded),
            entry("/w/a/build", DefaultExcluded),
            entry("/w/x.log", Ignored),
        ],
        true,
        false,
    )
    .expect("valid")
    .expect("entries");
    let listed = shown
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.cause))
        .collect::<Vec<_>>();
    assert_eq!(
        listed,
        [
            ("/w/a/build", IgnoredDefaultExcluded),
            ("/w/.x", Hidden),
            ("/w/node_modules", DefaultExcluded),
            ("/w/x.log", Ignored),
        ]
    );
}

/// An entry of several paths names the directory holding them all, whatever
/// its cause, so `path` has one meaning: where every path it counts sits. An
/// entry of one path names that path, and its words say which it is.
#[test]
fn a_group_of_several_paths_names_the_directory_holding_them() {
    use SkippedPathCause::{DefaultExcluded, Hidden, Ignored};
    let shown = skipped_paths(
        [
            SkippedEntry::path("/w/a/bundle.min.js", DefaultExcluded),
            SkippedEntry::path("/w/a/b/blob.min.js", DefaultExcluded),
            SkippedEntry::path("/w/a/.cache", Hidden),
            SkippedEntry::path("/w/c/.env.local", Hidden),
            SkippedEntry::path("/w/target", Ignored),
        ],
        false,
        false,
    )
    .expect("valid")
    .expect("entries");
    let listed = shown
        .entries
        .iter()
        .map(|entry| (entry.cause, entry.path.as_str(), entry.count))
        .collect::<Vec<_>>();
    assert_eq!(
        listed,
        [
            (DefaultExcluded, "/w/a", Some(2)),
            (Hidden, "/w", Some(2)),
            (Ignored, "/w/target", None),
        ]
    );
    assert_eq!(
        disclosure(&shown.entries[0], "a"),
        "2 default-excluded under `a`"
    );
    assert_eq!(
        disclosure(&shown.entries[2], "target"),
        "`target` (ignored)"
    );
}

/// Default-excluded paths carry the rule that left them out: a pruned
/// directory its name, a generated file the pattern it matches. Entries of
/// one rule group under the directory holding them; two rules stay apart.
#[test]
fn default_excluded_entries_name_their_rule() {
    let root = std::path::Path::new("/w");
    let pruned = ["node_modules/".to_owned(), "pkg/node_modules/".to_owned()];
    let generated = ["dist/app.min.js".to_owned(), "blob.bin".to_owned()];
    let shown = skipped_paths(default_excluded(root, &pruned, &generated), false, false)
        .expect("valid")
        .expect("entries");
    let listed = shown
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.reason.as_deref().unwrap_or_default(),
                entry.count,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        listed,
        [
            ("/w", "node_modules/", Some(2)),
            ("/w/dist/app.min.js", "*.min.js", None),
            ("/w/blob.bin", "*.bin", None),
        ]
    );
}

/// A few unparsed files are named one by one; past [`NAMED_UNPARSED`] they
/// group by type under the directory holding them, with their count.
#[test]
fn many_unparsed_files_group_by_type() {
    let entries = |count: usize| {
        (0..count)
            .map(|n| {
                SkippedEntry::path(format!("/w/data/item{n}.entry"), SkippedPathCause::Unparsed)
                    .because("entry")
            })
            .collect::<Vec<_>>()
    };
    let few = unparsed_paths(entries(3), false)
        .expect("paths")
        .expect("some");
    assert_eq!(few.entries.len(), 3);
    let many = unparsed_paths(entries(NAMED_UNPARSED + 5), false)
        .expect("paths")
        .expect("some");
    assert_eq!(many.entries.len(), 1, "{:?}", many.entries);
    assert_eq!(many.entries[0].count, Some(25));
    assert_eq!(many.entries[0].path.as_str(), "/w/data");
}

/// Files at `paths` under `root`, each written with one line.
fn files(root: &std::path::Path, paths: &[&str]) {
    for path in paths {
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().expect("parent")).expect("dir");
        std::fs::write(at, "x\n").expect("file");
    }
}

/// A cause whose paths spread across a walk is named per top directory of
/// the walk, never only as its root: an ignored `node_modules/` at the root
/// and the ignored `dist/` output of several packages are separate entries,
/// each `ignored-default-excluded` (lifting one flag alone does not reach
/// it), apart from the other ignored files there. An entry holding no file
/// cannot hold a match: it is not named, only counted at the root.
#[test]
fn a_spread_cause_is_named_per_top_directory() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path();
    let deps = (0..30)
        .map(|n| format!("node_modules/p{n}/index.js"))
        .collect::<Vec<_>>();
    files(root, &deps.iter().map(String::as_str).collect::<Vec<_>>());
    files(
        root,
        &[
            "packages/a/dist/out.js",
            "packages/b/dist/out.js",
            "packages/c/out.log",
            ".cache/f",
        ],
    );
    std::fs::create_dir_all(root.join("releases")).expect("empty dir");
    let mut walked = octocode_runtime_base::policy::discovery::WalkSkips::default();
    for path in [
        "node_modules/",
        "packages/a/dist/",
        "packages/b/dist/",
        "packages/c/out.log",
        "releases/",
    ] {
        walked.ignored.insert(path.to_owned());
    }
    walked.ignored_excluded = true;
    walked.hidden.insert(".cache/".to_owned());
    let shown = scoped_ledger(root, &walked, WalkErrors::default(), false)
        .expect("valid")
        .expect("entries");
    let at = |path: &str| root.join(path).to_string_lossy().into_owned();
    let entries = shown
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str().to_owned(),
                entry.cause,
                entry.count.unwrap_or(1),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [
            (
                at("node_modules"),
                SkippedPathCause::IgnoredDefaultExcluded,
                1
            ),
            (at("packages"), SkippedPathCause::IgnoredDefaultExcluded, 2),
            (at("packages/c/out.log"), SkippedPathCause::Ignored, 1),
            (at(".cache"), SkippedPathCause::Hidden, 1),
            // The empty `releases/`, counted at the root.
            (
                root.to_string_lossy().into_owned(),
                SkippedPathCause::Ignored,
                1
            ),
        ],
        "{shown:?}"
    );
    assert_eq!(shown.entries[0].reason.as_deref(), Some("node_modules/"));
    assert_eq!(shown.entries[1].reason.as_deref(), Some("dist/"));
    // The same ledger grouped per cause names only the root.
    let flat = skipped_paths(
        ledger_entries(root, &walked, WalkErrors::default()),
        false,
        false,
    )
    .expect("valid")
    .expect("entries");
    assert_eq!(
        flat.entries[0].path.as_str(),
        root.to_string_lossy(),
        "{flat:?}"
    );
}

/// At most [`NAMED_TOP`] entries are named, those holding the most files
/// first; every other one is counted at the root, so a walk that skipped
/// many places states a few entries.
#[test]
fn a_scoped_disclosure_names_the_fullest_entries_up_to_its_cap() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path();
    let tops = NAMED_TOP + 4;
    let mut walked = octocode_runtime_base::policy::discovery::WalkSkips::default();
    for top in 0..tops {
        std::fs::create_dir_all(root.join(format!(".t{top:02}"))).expect("dir");
        let held = (0..top)
            .map(|file| format!(".t{top:02}/f{file}"))
            .collect::<Vec<_>>();
        files(root, &held.iter().map(String::as_str).collect::<Vec<_>>());
        walked.hidden.insert(format!(".t{top:02}/"));
    }
    let shown = scoped_ledger(root, &walked, WalkErrors::default(), false)
        .expect("valid")
        .expect("entries");
    assert_eq!(shown.entries.len(), NAMED_TOP + 1, "{shown:?}");
    let named = shown.entries[..NAMED_TOP]
        .iter()
        .map(|entry| {
            std::path::Path::new(entry.path.as_str())
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    let fullest = (tops - NAMED_TOP..tops)
        .rev()
        .map(|top| format!(".t{top:02}"))
        .collect::<Vec<_>>();
    assert_eq!(named, fullest, "{shown:?}");
    let rest = &shown.entries[NAMED_TOP];
    assert_eq!(rest.path.as_str(), root.to_string_lossy());
    assert_eq!(rest.count, Some(4), "{shown:?}");
}
