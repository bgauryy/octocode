//! Comparing a tag with the ranges of records OSV cannot match by version.
use super::*;
use crate::security_check::ecosystem::Ecosystem;
use octocode_test_support::json_fixture;
use serde_json::json;

fn record(id: &str, affected: Value) -> Value {
    json!({"id": id, "affected": [{
        "package": {"name": "o/action", "ecosystem": "GitHub Actions"},
    }]})
    .as_object()
    .map(|base| {
        let mut base = base.clone();
        let entry = base["affected"][0].as_object().cloned().unwrap_or_default();
        let mut entry = entry;
        entry.extend(affected.as_object().cloned().unwrap_or_default());
        base.insert("affected".into(), json!([entry]));
        Value::Object(base)
    })
    .expect("record")
}

fn ranged(events: Value) -> Value {
    json!({"ranges": [{"type": "ECOSYSTEM", "events": events}]})
}

fn at(tag: &str, records: Vec<Value>) -> Narrowed {
    let names = ["o/action".to_owned()];
    narrow(
        records,
        &Selection::Named(Ecosystem::GitHubActions, &names, None),
        tag,
    )
}

fn ids(narrowed: &Narrowed) -> Vec<&str> {
    narrowed
        .kept
        .iter()
        .map(|record| record["id"].as_str().expect("id"))
        .collect()
}

#[test]
fn a_tag_inside_a_range_matches_and_one_at_its_fix_does_not() {
    let records = || {
        vec![record(
            "A",
            ranged(json_fixture(
                r#"[{"introduced": "0"}, {"fixed": "46.0.1"}]"#,
            )),
        )]
    };
    for tag in ["40.0.0", "v40.0.0", "46.0.0", "45.9.9"] {
        assert_eq!(ids(&at(tag, records())), ["A"], "{tag}");
    }
    for tag in ["46.0.1", "v46.0.1", "47.0.0", "v47.0.0"] {
        assert!(at(tag, records()).kept.is_empty(), "{tag}");
    }
    assert!(at("40.0.0", records()).unmatched.is_empty());
}

#[test]
fn later_ranges_and_inclusive_ends_follow_osvs_event_rules() {
    let events = json_fixture(
        r#"[
            {"introduced": "1.0.0"},
            {"fixed": "1.2.0"},
            {"introduced": "2.0.0"},
            {"last_affected": "2.3.0"}
        ]"#,
    );
    let records = || vec![record("A", ranged(events.clone()))];
    for (tag, hit) in [
        ("0.9.0", false),
        ("1.1.0", true),
        ("1.2.0", false),
        ("1.9.0", false),
        ("2.0.0", true),
        ("2.3.0", true),
        ("2.3.1", false),
    ] {
        assert_eq!(!at(tag, records()).kept.is_empty(), hit, "{tag}");
    }
    // Events out of order are sorted first, as OSV does.
    let shuffled = json_fixture(r#"[{"fixed": "1.2.0"}, {"introduced": "1.0.0"}]"#);
    assert_eq!(
        ids(&at("1.1.0", vec![record("A", ranged(shuffled))])),
        ["A"]
    );
}

#[test]
fn a_record_listing_versions_matches_exactly_those() {
    let listed = || {
        vec![record(
            "A",
            json_fixture(r#"{"versions": ["1.0.0", "v2"]}"#),
        )]
    };
    assert_eq!(ids(&at("v1.0.0", listed())), ["A"]);
    assert_eq!(ids(&at("2.0.0", listed())), ["A"]);
    assert!(at("3.0.0", listed()).kept.is_empty());
}

/// Nothing is dropped on a guess: an unreadable range or tag is kept and named.
#[test]
fn what_cannot_be_compared_is_kept_and_named_unmatched() {
    let unreadable = at(
        "40.0.0",
        vec![record(
            "A",
            ranged(json_fixture(
                r#"[{"introduced": "0"}, {"fixed": "nightly"}]"#,
            )),
        )],
    );
    assert_eq!(ids(&unreadable), ["A"]);
    assert_eq!(unreadable.unmatched, ["A"]);
    let branch = at(
        "main",
        vec![record(
            "B",
            ranged(json_fixture(r#"[{"introduced": "0"}, {"fixed": "2"}]"#)),
        )],
    );
    assert_eq!(ids(&branch), ["B"]);
    assert_eq!(branch.unmatched, ["B"]);
    // A record that states nothing about the package is unmatched too.
    let silent = at("1.0.0", vec![record("C", json!({}))]);
    assert_eq!(silent.unmatched, ["C"]);
    // A git range alone gives nothing to compare.
    let git = json_fixture(
        r#"{
            "ranges": [
                {
                    "type": "GIT",
                    "repo": "https://x",
                    "events": [{"introduced": "0"}]
                }
            ]
        }"#,
    );
    assert_eq!(at("1.0.0", vec![record("D", git)]).unmatched, ["D"]);
}

/// A tag short of its patch (`v4`) floats to the newest release of that line, so
/// it is no version to compare: a range that opens or closes inside the line
/// neither matches nor clears it.
#[test]
fn a_floating_tag_is_listed_unmatched_never_cleared() {
    let inside = || {
        vec![record(
            "A",
            ranged(json_fixture(
                r#"[{"introduced": "4.1.0"}, {"fixed": "4.2.0"}]"#,
            )),
        )]
    };
    for tag in ["v4", "4", "v4.1", "4.1"] {
        let narrowed = at(tag, inside());
        assert_eq!(ids(&narrowed), ["A"], "{tag}");
        assert_eq!(narrowed.unmatched, ["A"], "{tag}");
    }
    // The full version is compared as before.
    assert!(at("v4.0.0", inside()).kept.is_empty());
    assert_eq!(ids(&at("v4.1.5", inside())), ["A"]);
    assert!(at("v4.1.5", inside()).unmatched.is_empty());
}

/// Semver precedence ignores build metadata, so a build of the last affected
/// version is that version.
#[test]
fn build_metadata_does_not_change_precedence() {
    let last = || {
        vec![record(
            "A",
            ranged(json_fixture(
                r#"[{"introduced": "0"}, {"last_affected": "1.2.3"}]"#,
            )),
        )]
    };
    for tag in ["1.2.3", "1.2.3+build.7", "v1.2.3+sha.abc"] {
        assert_eq!(ids(&at(tag, last())), ["A"], "{tag}");
    }
    assert!(at("1.2.4+build", last()).kept.is_empty());
    // The record's own versions carry build metadata the same way.
    let tagged = || {
        vec![record(
            "B",
            ranged(json_fixture(
                r#"[{"introduced": "0"}, {"fixed": "2.0.0+build"}]"#,
            )),
        )]
    };
    assert_eq!(ids(&at("2.0.0-rc.1", tagged())), ["B"]);
    assert!(at("2.0.0", tagged()).kept.is_empty());
}

#[test]
fn a_record_version_reads_as_semver_with_missing_parts_zero() {
    for (tag, parsed) in [
        ("v1", "1.0.0"),
        ("1.2", "1.2.0"),
        ("1.2.3-rc.1", "1.2.3-rc.1"),
    ] {
        assert_eq!(parse(tag), Version::parse(parsed).ok(), "{tag}");
    }
    for tag in ["main", "1.2.3.4", "1..2", "", "v"] {
        assert_eq!(parse(tag), None, "{tag}");
    }
}

/// A repository advisory's written range clears the version only when every
/// entry's comparators are read and leave it out.
#[test]
fn a_written_range_clears_only_a_version_it_reads_and_leaves_out() {
    let entry = |range: &str| json!({"ecosystemSpecific": {"vulnerable_version_range": range}});
    for (range, tag, clear) in [
        ("< 4.3.9", "5.4.0", true),
        ("= 6.4.4", "5.4.0", true),
        (">= 6.0.0, <= 6.4.4", "5.4.0", true),
        (">=5.0.0 <5.4.6", "5.4.0", false),
        ("<= 5.4.0", "5.4.0", false),
        ("< 5.0.0rc1", "5.4.0", false),
        ("< 6.0.0", "v5", false),
    ] {
        assert_eq!(
            written_ranges_clear(&[entry(range)], tag),
            clear,
            "{range} at {tag}"
        );
    }
    assert!(!written_ranges_clear(
        &[json_fixture(r#"{"packageName": "x"}"#)],
        "5.4.0"
    ));
    assert!(!written_ranges_clear(&[], "5.4.0"));
    assert!(!written_ranges_clear(
        &[entry("< 4.3.9"), entry(">= 5.0.0, < 5.4.6")],
        "5.4.0"
    ));
}
