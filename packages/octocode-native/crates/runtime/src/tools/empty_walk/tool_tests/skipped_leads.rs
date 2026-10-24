//! The scenario table: a match behind a skipped path is named and led to;
//! nothing behind the skipped paths leads nowhere.

use super::*;

#[test]
fn a_match_behind_a_skipped_path_is_named_and_its_lead_lifts_only_that() {
    for tool in tools() {
        for scenario in SCENARIOS {
            let label = describe(&tool, scenario);
            let root = repo(scenario, "needle.ts", "function needle() {}\n");
            let query = (tool.query)(&root.0);
            let out = (tool.run)(&root.0, &query);
            let effective = FLAGGED
                .into_iter()
                .filter(|cause| scenario.causes.contains(cause) && tool.skips.contains(cause))
                .collect::<Vec<_>>();
            if effective.is_empty() {
                assert!((tool.found)(&out), "{label}: walked without a lead: {out}");
                continue;
            }
            assert!(
                !(tool.found)(&out),
                "{label}: found without the lead: {out}"
            );
            let lead = skipped_query(&out).unwrap_or_else(|| panic!("{label}: no lead: {out}"));
            // Validates against the contract.
            crate::contracts::validate(
                if tool.name.starts_with("astSearch") {
                    "astSearch"
                } else if tool.name.starts_with("structure") {
                    "structureSearch"
                } else {
                    tool.name
                },
                json!({"queries": [lead.clone()]}),
            )
            .unwrap_or_else(|error| panic!("{label}: lead invalid {lead}: {:?}", error.issues));
            for cause in FLAGGED
                .into_iter()
                .filter(|cause| tool.skips.contains(cause))
            {
                assert_eq!(
                    (tool.lifts)(&lead, cause, &root.0, scenario),
                    effective.contains(&cause),
                    "{label}: lead should lift exactly {effective:?}: {lead}"
                );
            }
            let skipped_directory = scenario.dir.split('/').next().expect("directory");
            assert!(
                out["skippedPaths"].to_string().contains(skipped_directory)
                    || prose(&out).contains(skipped_directory),
                "{label}: skipped directory undisclosed: {out}"
            );
            // The lead runs and finds the match.
            let found = (tool.run)(&root.0, &lead);
            assert!((tool.found)(&found), "{label}: lead found nothing: {found}");
        }
    }
}

#[test]
fn nothing_behind_the_skipped_paths_leads_nowhere() {
    for tool in tools() {
        for scenario in SCENARIOS {
            let label = describe(&tool, scenario);
            let root = repo(scenario, "other.md", "nothing to find\n");
            let query = (tool.query)(&root.0);
            let out = (tool.run)(&root.0, &query);
            assert!(!(tool.found)(&out), "{label}: {out}");
            assert!(
                skipped_query(&out).is_none(),
                "{label}: unneeded skipped-scope search: {out}"
            );
        }
    }
}
