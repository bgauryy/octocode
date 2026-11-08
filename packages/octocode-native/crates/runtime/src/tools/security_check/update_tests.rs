//! The update fields of a row, from the version comparator.
use super::*;

fn latest(version: &str) -> Latest {
    Latest {
        version: version.to_owned(),
        published_at: Some("2026-01-02".to_owned()),
    }
}

fn row(current: &str, latest_version: &str) -> Value {
    let mut data = json!({});
    apply(
        &mut data,
        current,
        Some("2020-01-01"),
        &latest(latest_version),
    );
    data
}

#[test]
fn each_kind_is_named_by_its_first_differing_component() {
    for (current, newest, kind) in [
        ("4.17.15", "4.17.21", "patch"),
        ("4.17.15", "4.18.0", "minor"),
        ("v1.9.9", "v2.0.0", "major"),
        ("1.0", "1.0.1", "patch"),
    ] {
        let data = row(current, newest);
        assert_eq!(data["upgradeKind"], kind, "{current} -> {newest}");
        assert_eq!(data["outdated"], true);
        assert_eq!(data["isLatest"], false);
    }
}

#[test]
fn the_latest_version_is_not_outdated_and_names_no_upgrade() {
    for (current, newest) in [("4.18.1", "4.18.1"), ("4.18", "4.18.0")] {
        let data = row(current, newest);
        assert_eq!(data["isLatest"], true, "{current}");
        assert_eq!(data["outdated"], false);
        assert!(data.get("upgradeKind").is_none(), "{data}");
    }
    assert_eq!(row("4.18.1", "4.18.1")["latestPublishedAt"], "2026-01-02");
    assert_eq!(row("4.18.1", "4.18.1")["currentPublishedAt"], "2020-01-01");
}

#[test]
fn a_version_that_is_not_dotted_numbers_is_unknown_never_guessed() {
    let data = row("1.0.0-beta.1", "1.0.0");
    assert_eq!(data["upgradeKind"], "unknown");
    assert_eq!(data["isLatest"], false);
    assert!(data.get("outdated").is_none(), "{data}");
    let same = row("2024.1a", "2024.1a");
    assert_eq!(same["isLatest"], true);
    assert!(same.get("outdated").is_none(), "{same}");
}

#[test]
fn a_version_past_the_latest_is_not_outdated() {
    let data = row("5.0.0", "4.18.1");
    assert_eq!(data["outdated"], false);
    assert_eq!(data["isLatest"], false);
}
