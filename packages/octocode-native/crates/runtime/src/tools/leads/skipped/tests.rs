use super::*;
use crate::tools::empty_walk::Walk;

const ENTRIES: [&str; 2] = ["entry", "entries"];

fn verdict(lifted: &[Cause], count: usize) -> Verdict {
    let root = std::path::Path::new("/w");
    crate::tools::empty_walk::diagnose(
        root,
        Walk {
            ignored: false,
            hidden: false,
            default_excludes: true,
        },
        |walk| {
            let reached = lifted.iter().all(|cause| !walk.skipping().contains(cause));
            Ok::<_, ()>(crate::tools::empty_walk::Hits {
                count: if reached { count } else { 0 },
                sample: if reached {
                    (0..count)
                        .map(|index| root.join(format!(".h/{index}.rs")))
                        .collect()
                } else {
                    Vec::new()
                },
                cut: false,
            })
        },
    )
    .expect("diagnosis")
}

/// A lead is the query's search, not the caller's words about it: the
/// brief fields are the caller's to restate for the new call.
#[test]
fn a_lead_repeats_the_search_without_the_callers_brief() {
    let query = serde_json::json!({
        "path": "src",
        "matchString": ["needle"],
        "mainGoal": "the task",
        "reasoning": "why this query",
    });
    let lead = lead(ToolId::LocalSearch, &query, &[Cause::Hidden]);
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["path"], "src", "{lead}");
    assert_eq!(row["includeHidden"], true, "{lead}");
    assert!(row.get("mainGoal").is_none(), "{lead}");
    assert!(row.get("reasoning").is_none(), "{lead}");
}

/// One sentence shape whatever the tool: the count, the subject and the
/// reasons (the lead sits in `next`); a cut check is a warning that leaves the
/// result partial and lifts every cause the walk skipped by.
#[test]
fn every_tool_discloses_skipped_paths_in_one_shape() {
    let none = Unchecked {
        causes: &[],
        named: "",
    };
    let found = disclose(&verdict(&[Cause::Hidden], 2), ENTRIES, none).expect("found");
    assert_eq!(found.causes, [Cause::Hidden]);
    assert_eq!(found.text, "2 entries behind skipped paths (hidden .h/).");
    assert!(!found.unchecked);
    assert_eq!(disclose(&Verdict::Absent, ENTRIES, none), None);
    assert_eq!(disclose(&Verdict::Unproven, ENTRIES, none), None);
    let cut = disclose(
        &Verdict::Unproven,
        ENTRIES,
        Unchecked {
            causes: &[Cause::Ignored, Cause::DefaultExcluded],
            named: "3 ignored",
        },
    )
    .expect("cut");
    assert!(cut.unchecked);
    assert_eq!(cut.causes, [Cause::Ignored, Cause::DefaultExcluded]);
    let mut out = serde_json::json!({});
    attach(
        &mut out,
        &cut,
        lead(
            ToolId::StructureSearch,
            &serde_json::json!({"operation": "files", "path": ".", "page": 3, "snapshot": "x"}),
            &cut.causes,
        ),
    );
    assert!(
        out.get("partialReasons").is_none(),
        "the requested walk was whole"
    );
    assert!(
        out["warnings"][0]
            .as_str()
            .expect("warning")
            .contains("unproven")
    );
    assert_eq!(out["calls"]["includeSkipped"]["kind"], "related");
    let row = &out["calls"]["includeSkipped"]["input"]["queries"][0];
    assert_eq!(row["includeIgnored"], true, "{row}");
    assert_eq!(row["defaultExcludes"], false, "{row}");
    assert!(
        row.get("page").is_none() && row.get("snapshot").is_none(),
        "{row}"
    );
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

/// Withheld entries are counted by rule at the deepest directory holding
/// them all, one entry per rule, never one per holding directory; with every
/// path kept, each directory stays its own entry. Every count is kept.
#[test]
fn withheld_entries_are_one_counted_entry_per_rule() {
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
    assert_eq!(credentials.len(), 1, "{grouped:?}");
    assert_eq!(credentials[0].path.as_str(), "/w/ext", "{grouped:?}");
    assert!(grouped.sampled, "{grouped:?}");
    let every = skipped_paths(entries.clone(), true, false)
        .expect("valid")
        .expect("entries");
    assert_eq!(every.entries.len(), entries.len(), "{every:?}");
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
