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
    assert_eq!(
        out["partialReasons"],
        serde_json::json!([SKIPPED_UNCHECKED])
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
