//! Which version clears the listed advisories, and when none can be named.
use super::*;

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

#[test]
fn the_highest_of_each_advisorys_lowest_fix_above_the_current_version() {
    let rows = [
        fixed("A", &["4.17.21"]),
        fixed("B", &["4.17.23"]),
        fixed("C", &["4.17.19"]),
    ];
    // C has no fix above the current version: no version clears it.
    assert_eq!(candidate("4.17.20", &rows[..2]), Ok("4.17.23".to_owned()));
    assert!(
        candidate("4.17.20", &rows)
            .unwrap_err()
            .contains("C states no fixed")
    );
    // Two branches: the fix on the current one, not the newest.
    let branches = [
        fixed("A", &["2.2.14", "3.1.18"]),
        fixed("B", &["2.2.4", "3.0.0"]),
    ];
    assert_eq!(candidate("2.2.3", &branches), Ok("2.2.14".to_owned()));
    // Numbers, not text: 2.2.10 is above 2.2.9.
    assert_eq!(
        candidate("2.2.3", &[fixed("A", &["2.2.9"]), fixed("B", &["2.2.10"])]),
        Ok("2.2.10".to_owned())
    );
}

#[test]
fn it_writes_the_version_as_the_current_one_does_and_skips_prereleases() {
    assert_eq!(
        candidate("v0.3.5", &[fixed("A", &["0.3.8"])]),
        Ok("v0.3.8".to_owned())
    );
    assert_eq!(
        candidate("1.0.0", &[fixed("A", &["2.0.0-rc1", "1.5.0"])]),
        Ok("1.5.0".to_owned())
    );
}

#[test]
fn it_says_why_when_no_version_can_be_named() {
    let why = candidate("1.0.0", &[fixed("A", &[])]).unwrap_err();
    assert!(why.contains("A states no fixed version"), "{why}");
    let why = candidate("3.0.0-beta3", &[fixed("A", &["3.0.0"])]).unwrap_err();
    assert!(why.contains("not a plain dotted version"), "{why}");
    let why = candidate("1.0.0", &[fixed("A", &["2.0.0-rc1"])]).unwrap_err();
    assert!(why.contains("A states no fixed version"), "{why}");
}
