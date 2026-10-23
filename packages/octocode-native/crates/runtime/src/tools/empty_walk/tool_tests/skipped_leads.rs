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
            let lead = lead_query(&out, (tool.channel)(scenario))
                .unwrap_or_else(|| panic!("{label}: no lead: {out}"));
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
            // The hint names the deciding rule, entry or directory.
            let first = effective[0];
            let reason = scenario
                .reasons
                .iter()
                .find(|(cause, _)| *cause == first)
                .map(|(_, reason)| *reason)
                .expect("scenario names the cause");
            if tool.name == "localSearch" {
                let entry_path = scenario.dir.split('/').next().expect("directory");
                assert!(
                    out["skippedPaths"]["entries"]
                        .as_array()
                        .expect("skipped entries")
                        .iter()
                        .any(|entry| entry["path"]
                            == json!(
                                root.0
                                    .canonicalize()
                                    .expect("canonical fixture")
                                    .join(entry_path)
                            )
                            && entry["cause"] == first.label()
                            && entry["reason"] == reason),
                    "{label}: exact skipped path, cause and `{reason}` in {out}"
                );
            } else {
                assert!(prose(&out).contains(reason), "{label}: `{reason}` in {out}");
            }
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
            for channel in ["includeSkipped", "narrowScope"] {
                assert!(
                    lead_query(&out, channel).is_none(),
                    "{label}: lead to nothing: {out}"
                );
            }
            if tool.prose_when_nothing_found {
                assert!(!prose(&out).is_empty(), "{label}: no fallback hint: {out}");
            }
        }
    }
}
