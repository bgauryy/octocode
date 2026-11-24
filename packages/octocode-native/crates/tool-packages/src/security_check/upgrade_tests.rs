//! Which version clears the listed advisories, and when none can be named.
use super::*;
use octocode_contract_types::ArtifactType;

fn fixed(id: &str, versions: &[&str]) -> SecurityAdvisory {
    SecurityAdvisory {
        id: id.to_owned(),
        fixed_in: versions
            .iter()
            .map(|version| (*version).to_owned())
            .collect(),
        ..SecurityAdvisory::default()
    }
}

/// Dotted numbers only: an ecosystem no registry orders.
fn dotted() -> Order {
    Order::of(None)
}

/// npm's order: Semantic Versioning precedence.
fn npm() -> Order {
    Order::of(Some(crate::artifact::order_of(ArtifactType::Npm)))
}

#[test]
fn the_highest_of_each_advisorys_lowest_fix_above_the_current_version() {
    for order in [dotted(), npm()] {
        let rows = [
            fixed("A", &["4.17.21"]),
            fixed("B", &["4.17.23"]),
            fixed("C", &["4.17.19"]),
        ];
        // C has no fix above the current version: no version clears it.
        assert_eq!(
            candidate("4.17.20", &rows[..2], order),
            Ok("4.17.23".to_owned())
        );
        assert!(
            candidate("4.17.20", &rows, order)
                .unwrap_err()
                .contains("C states no fixed")
        );
        // Two branches: the fix on the current one, not the newest.
        let branches = [
            fixed("A", &["2.2.14", "3.1.18"]),
            fixed("B", &["2.2.4", "3.0.0"]),
        ];
        assert_eq!(
            candidate("2.2.3", &branches, order),
            Ok("2.2.14".to_owned())
        );
        // Numbers, not text: 2.2.10 is above 2.2.9.
        assert_eq!(
            candidate(
                "2.2.3",
                &[fixed("A", &["2.2.9"]), fixed("B", &["2.2.10"])],
                order
            ),
            Ok("2.2.10".to_owned())
        );
    }
}

#[test]
fn it_writes_the_version_as_the_current_one_does_and_counts_prereleases() {
    assert_eq!(
        candidate("v0.3.5", &[fixed("A", &["0.3.8"])], npm()),
        Ok("v0.3.8".to_owned())
    );
    assert_eq!(
        candidate("1.0.0", &[fixed("A", &["2.0.0-rc1", "1.5.0"])], npm()),
        Ok("1.5.0".to_owned())
    );
    // A prerelease is the lowest fix when it is the only one listed.
    assert_eq!(
        candidate("1.0.0", &[fixed("A", &["2.0.0-rc1"])], npm()),
        Ok("2.0.0-rc1".to_owned())
    );
}

/// A canary-only fix counts in the registry's order: the highest of the
/// lowest fixes still decides (next 13.4.0, parse-server 5.0.0).
#[test]
fn a_prerelease_fix_counts_in_the_registry_order() {
    let next = [
        fixed("GHSA-c59h-r6p8-q9wc", &["13.4.20-canary.13"]),
        fixed("GHSA-2464-fhr4-w9xw", &["15.5.28"]),
    ];
    assert_eq!(candidate("13.4.0", &next, npm()), Ok("15.5.28".to_owned()));
    let parse = [
        fixed("GHSA-7cx5-254x-cgrq", &["8.5.0-alpha.5"]),
        fixed("GHSA-r899-h629-j84r", &["9.10.0-alpha.2", "8.6.84"]),
    ];
    assert_eq!(candidate("5.0.0", &parse, npm()), Ok("8.6.84".to_owned()));
}

#[test]
fn it_says_why_when_no_version_can_be_named() {
    let why = candidate("1.0.0", &[fixed("A", &[])], npm()).unwrap_err();
    assert!(why.contains("A states no fixed version"), "{why}");
    // Dotted numbers read no prerelease: neither as the version nor a fix.
    let why = candidate("3.0.0-beta3", &[fixed("A", &["3.0.0"])], dotted()).unwrap_err();
    assert!(why.contains("not a version"), "{why}");
    let why = candidate("1.0.0", &[fixed("A", &["2.0.0-rc1"])], dotted()).unwrap_err();
    assert!(why.contains("A states no fixed version"), "{why}");
    // The registry's order reads them both.
    assert_eq!(
        candidate("3.0.0-beta3", &[fixed("A", &["3.0.0"])], npm()),
        Ok("3.0.0".to_owned())
    );
}

/// An upgrade across a major says which advisories the current major leaves
/// unfixed and what the current major does fix; one inside the major, or
/// a current major that clears everything, says nothing.
#[test]
fn a_cross_major_upgrade_names_what_the_current_major_leaves_unfixed() {
    let order = npm();
    let rows = [
        fixed("A", &["6.5.8", "7.4.5"]),
        fixed("B", &["7.4.5", "7.15.2"]),
    ];
    assert_eq!(candidate("6.3.0", &rows, order), Ok("7.4.5".to_owned()));
    let note = major_gap("6.3.0", "7.4.5", &rows, order).expect("gap");
    assert!(
        note.contains("No 6.x release is listed as fixing B"),
        "{note}"
    );
    assert!(
        note.contains("7.4.5 is the lowest listed fix in a later major"),
        "{note}"
    );
    assert!(
        note.contains("6.5.8 is the highest fix listed within 6.x"),
        "{note}"
    );
    // Nothing of the current major to offer: only the first part.
    let note = major_gap("6.3.0", "7.4.5", &rows[1..], order).expect("gap");
    assert!(!note.contains("highest fix listed within"), "{note}");
    // Same major, or every advisory fixed in it: nothing to add.
    let within = [fixed("A", &["6.5.8", "7.4.5"]), fixed("B", &["6.5.9"])];
    assert_eq!(candidate("6.3.0", &within, order), Ok("6.5.9".to_owned()));
    assert_eq!(major_gap("6.3.0", "6.5.9", &within, order), None);
    assert_eq!(major_gap("6.3.0", "7.0.0", &within, order), None);
    assert!(major_gap("v6.3.0", "v7.4.5", &rows, order).is_some());
    // A prerelease of the current major is a fix listed within it.
    let canary = [fixed("C", &["13.4.20-canary.13"]), fixed("D", &["15.5.28"])];
    assert_eq!(unfixed_in_major("13.4.0", "15.5.28", &canary, order), ["D"]);
}

/// A package row states the one fix that decides for the asked version.
#[test]
fn the_fix_of_the_asked_versions_line_decides() {
    let fixes = |current: &str, listed: &[&str], order: Order| {
        let mut listed = listed.iter().map(|fix| (*fix).to_owned()).collect();
        deciding_fix(current, &mut listed, order);
        listed
    };
    let listed = ["6.2.6", "6.1.5", "5.4.18", "4.5.13", "5.4.0"];
    assert_eq!(fixes("5.4.1", &listed, npm()), ["5.4.18"]);
    // No fix in the line: the lowest in a later one.
    assert_eq!(
        fixes("5.4.1", &["8.0.5", "7.3.2", "6.4.2"], npm()),
        ["6.4.2"]
    );
    // Nothing to choose by: the list stays whole.
    assert_eq!(
        fixes("1.0.0-rc1", &["2.0.0", "1.5.3"], dotted()),
        ["2.0.0", "1.5.3"]
    );
    assert_eq!(fixes("9.0.0", &["2.0.0"], npm()), ["2.0.0"]);
    // The registry's order reads a prerelease, asked or listed.
    assert_eq!(fixes("1.0.0-rc1", &["2.0.0", "1.5.3"], npm()), ["1.5.3"]);
    assert_eq!(
        fixes("13.4.0", &["13.4.20-canary.13"], npm()),
        ["13.4.20-canary.13"]
    );
}

/// The release a walk steps to: the lowest above, stable unless asked.
#[test]
fn the_next_published_release_is_stable_unless_prereleases_are_asked() {
    let published = ["0.3.31", "0.3.32-alpha.1", "0.3.8", "0.3.40"]
        .map(str::to_owned)
        .to_vec();
    assert_eq!(
        next_published("0.3.7", &published, npm(), false).as_deref(),
        Some("0.3.8")
    );
    assert_eq!(
        next_published("0.3.31", &published, npm(), false).as_deref(),
        Some("0.3.40")
    );
    assert_eq!(
        next_published("0.3.31", &published, npm(), true).as_deref(),
        Some("0.3.32-alpha.1")
    );
    assert_eq!(next_published("0.3.40", &published, npm(), true), None);
    assert!(npm().same("v1.2.3", "1.2.3"));
}

/// An advisory's records each close the asked version's line at their own
/// fix: the advisory is clear at the highest of them, in the registry's
/// order; a fix not above the version, or one the order cannot read, keeps
/// the list whole.
#[test]
fn an_advisory_clears_at_the_highest_of_its_records_fixes() {
    let clearing = |current: &str, listed: &[&str], order: Order| {
        let mut listed = listed.iter().map(|fix| (*fix).to_owned()).collect();
        clearing_fix(current, &mut listed, order);
        listed
    };
    assert_eq!(clearing("0.1.45", &["0.2.23", "0.2.0"], npm()), ["0.2.23"]);
    assert_eq!(
        clearing("4.17.20", &["4.18.0", "4.18.1"], npm()),
        ["4.18.1"]
    );
    assert_eq!(
        clearing("1.0.0", &["1.10.0", "1.9.0"], dotted()),
        ["1.10.0"]
    );
    assert_eq!(clearing("4.17.20", &["4.18.0"], npm()), ["4.18.0"]);
    assert_eq!(
        clearing("4.17.20", &["4.18.0", "4.0.0"], npm()),
        ["4.18.0", "4.0.0"]
    );
    assert_eq!(
        clearing("4.17.20", &["4.18.0", "next"], npm()),
        ["4.18.0", "next"]
    );
}
