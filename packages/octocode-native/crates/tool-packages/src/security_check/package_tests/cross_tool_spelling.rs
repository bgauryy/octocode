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

/// One spelling on both tools, for every registry artifactSearch reads: the
/// word it takes runs on securityCheck as sent, OSV is asked by OSV's own
/// name of it, and what a row states back (an advisory's affected package,
/// the row a walk's next page carries) is that word again.
#[test]
fn an_artifact_search_ecosystem_runs_here_as_sent_and_comes_back_unchanged() {
    use crate::artifact::query::artifact_type::{as_str, osv_ecosystem};
    use crate::security_check::{ecosystem, record};
    let contract =
        octocode_runtime_base::contracts::security::security_contract().expect("contract");
    let kinds = artifact_ecosystems();
    assert!(!kinds.is_empty());
    for kind in kinds {
        let word = as_str(kind);
        runs_as_sent(
            "securityCheck",
            &json!({"queries": [{"operation": "package", "ecosystem": word, "packageName": "x"}]}),
        );
        let typed = osv_ecosystem(kind);
        assert_eq!(typed.to_string(), word);
        // What OSV is asked: the contract's OSV name of the word.
        let osv = contract
            .ecosystems
            .osv_names
            .get(word)
            .expect("an OSV name");
        assert_eq!(ecosystem::osv_name(typed, None), *osv, "{word}");
        // What OSV states comes back as the word.
        let stated =
            json!({"id": "X-1", "affected": [{"package": {"name": "x", "ecosystem": osv}}]});
        let shown = record::advisory(&stated, &record::Selection::All).expect("record");
        assert_eq!(shown.affected[0]["ecosystem"], json!(word), "{word}");
        assert_eq!(ecosystem::as_input(osv), word);
        // The row a stopped walk reads on with names the word.
        let package = Package {
            view: octocode_contract_types::SecurityCheckView::Summary,
            ecosystem: typed,
            release: None,
            name: "x",
            version: None,
            resume: None,
            brief: Map::new(),
        };
        assert_eq!(package_row(&package, "x", None)["ecosystem"], json!(word));
    }
}

/// OSV's own spelling of a registry is never a silent input: it is read as
/// the word with a note that names the word (a case form of it), or refused
/// with the word to send.
#[test]
fn osvs_spelling_of_a_registry_names_the_word_to_send() {
    let contract =
        octocode_runtime_base::contracts::security::security_contract().expect("contract");
    for (word, osv) in contract
        .ecosystems
        .osv_names
        .iter()
        .filter(|(word, osv)| word != osv)
    {
        let input =
            json!({"queries": [{"operation": "package", "ecosystem": osv, "packageName": "x"}]});
        let (read, notes) =
            octocode_runtime_base::contracts::normalized_input("securityCheck", input);
        let quoted = format!("\"{word}\"");
        match octocode_runtime_base::contracts::validate("securityCheck", read.clone()) {
            Ok(_) => {
                assert_eq!(read["queries"][0]["ecosystem"], json!(word), "{osv}");
                assert!(notes.iter().any(|note| note.contains(&quoted)), "{notes:?}");
            }
            Err(refused) => {
                let said = refused.to_string();
                assert!(said.contains(&format!("use {quoted}")), "{osv}: {said}");
            }
        }
    }
}
