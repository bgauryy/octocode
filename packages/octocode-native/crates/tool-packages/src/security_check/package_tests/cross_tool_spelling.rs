//! The calls each package tool leads to in the other spell the ecosystem
//! as the target tool takes it, so following a lead never meets the input
//! repair's "read ecosystem … as …" note.
use super::*;
use crate::test_spelling::{artifact_ecosystems, runs_as_sent};

/// securityCheck's search lead for a name its registry lacks names the
/// ecosystem as artifactSearch spells it, for every ecosystem that has one.
#[test]
fn the_search_lead_spells_the_ecosystem_as_artifact_search_takes_it() {
    let mut led = 0;
    for kind in artifact_ecosystems() {
        let package = Package {
            view: octocode_contract_types::SecurityCheckView::Summary,
            ecosystem: crate::artifact::query::artifact_type::osv_ecosystem(kind),
            release: None,
            name: "no-such-package-x",
            version: None,
            resume: None,
            brief: Map::new(),
        };
        let error = not_on_registry(kind, &package, None);
        let Some(calls) = error.calls.as_deref() else {
            continue;
        };
        for call in calls.as_object().expect("calls").values() {
            if call["tool"] == "artifactSearch" {
                runs_as_sent("artifactSearch", &call["input"]);
                led += 1;
            }
        }
    }
    assert!(led > 0, "some ecosystem leads to a search");
}
