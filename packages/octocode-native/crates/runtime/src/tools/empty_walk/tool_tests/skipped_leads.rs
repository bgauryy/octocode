//! The scenario table: a skipped path is named and led to, whatever it
//! holds, and the lead reaches a match behind it. No tool reads behind a
//! skipped path to decide.
use serde_json::json;

use super::*;

#[test]
fn a_match_behind_a_skipped_path_is_named_and_its_lead_reaches_it() {
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
            octocode_runtime_base::contracts::validate(
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
                let lifted = (tool.lifts)(&lead, cause, &root.0, scenario);
                if effective.contains(&cause) {
                    assert!(lifted, "{label}: lead should lift {effective:?}: {lead}");
                }
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

/// Nothing is read behind a skipped path, so what it holds changes nothing:
/// an empty answer over a skipped directory names it and offers the lead.
#[test]
fn a_skipped_path_is_led_to_whatever_it_holds() {
    for tool in tools() {
        for scenario in SCENARIOS {
            let label = describe(&tool, scenario);
            let effective = FLAGGED
                .into_iter()
                .any(|cause| scenario.causes.contains(&cause) && tool.skips.contains(&cause));
            if !effective {
                continue;
            }
            let root = repo(scenario, "other.md", "nothing to find\n");
            let query = (tool.query)(&root.0);
            let out = (tool.run)(&root.0, &query);
            assert!(!(tool.found)(&out), "{label}: {out}");
            assert!(
                skipped_query(&out).is_some(),
                "{label}: no lead over the skipped paths: {out}"
            );
        }
    }
}
