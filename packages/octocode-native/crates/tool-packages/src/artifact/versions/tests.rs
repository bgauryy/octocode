//! Version specs, range resolution per ecosystem and nearest-version hints.
use super::*;

#[test]
fn classifies_exact_tag_and_range() {
    assert_eq!(
        VersionSpec::parse("3.22.0"),
        VersionSpec::Exact("3.22.0".into())
    );
    assert_eq!(
        VersionSpec::parse("v1.0.100"),
        VersionSpec::Exact("1.0.100".into())
    );
    assert_eq!(
        VersionSpec::parse("latest"),
        VersionSpec::Tag("latest".into())
    );
    assert_eq!(VersionSpec::parse("next"), VersionSpec::Tag("next".into()));
    for range in ["^3", "~3.22", ">=1.2 <2", "3.x", "1 || 2", "2.31"] {
        assert_eq!(
            VersionSpec::parse(range),
            VersionSpec::Range(range.into()),
            "{range}"
        );
    }
}

#[test]
fn npm_ranges_follow_npm_semantics() {
    let versions = ["3.22.0", "3.25.76", "3.26.0-beta.1", "4.0.0", "4.6.5"];
    let resolve = |range| npm_resolve(range, versions, Some("4.6.5"), |_| false);
    assert_eq!(resolve("^3").as_deref(), Some("3.25.76"));
    assert_eq!(resolve("~3.22").as_deref(), Some("3.22.0"));
    assert_eq!(resolve(">=3.22.0 <4").as_deref(), Some("3.25.76"));
    assert_eq!(resolve(">= 3.22.0 < 4").as_deref(), Some("3.25.76"));
    assert_eq!(resolve("3.x").as_deref(), Some("3.25.76"));
    assert_eq!(
        resolve("3.22").as_deref(),
        Some("3.22.0"),
        "npm: partial = x-range"
    );
    assert_eq!(resolve("3.22.0 - 3.30.0").as_deref(), Some("3.25.76"));
    assert_eq!(
        resolve("^2 || ^4").as_deref(),
        Some("4.6.5"),
        "latest satisfies"
    );
    assert_eq!(resolve("^5"), None);
}

#[test]
fn cargo_ranges() {
    assert_eq!(
        cargo_resolve("^1.0.100", ["1.0.99", "1.0.100", "1.0.228", "2.0.0"]).as_deref(),
        Some("1.0.228")
    );
}

/// What a repair call of `limit` rows reads of `versions` near `requested`.
fn suggest(
    ecosystem: ArtifactType,
    requested: &str,
    versions: &[&str],
    limit: usize,
) -> Vec<String> {
    let published = versions
        .iter()
        .map(|version| (*version).to_owned())
        .collect::<Vec<_>>();
    let error = version_not_found(ecosystem, "pkg", requested, &published);
    suggested(ecosystem, &error, limit).0
}

#[test]
fn nearest_versions_share_the_longest_prefix() {
    let versions = ["1.0.99", "1.0.100", "1.0.101", "1.1.0", "2.0.0"];
    assert_eq!(
        suggest(ArtifactType::Npm, "1.0.105", &versions, 3),
        ["1.0.101", "1.0.100", "1.0.99"]
    );
    // No shared prefix: the closest versions either side of it, newest first.
    assert_eq!(
        suggest(ArtifactType::Npm, "3.0.0", &versions, 3),
        ["2.0.0", "1.1.0", "1.0.101"]
    );
    assert_eq!(
        suggest(
            ArtifactType::Npm,
            "5.0.0",
            &["0.4.0", "4.9.1", "6.0.0", "6.1.0", "6.2.0", "7.0.0-rc.1"],
            4
        ),
        ["6.1.0", "6.0.0", "4.9.1", "0.4.0"]
    );
    assert_eq!(
        suggest(ArtifactType::Npm, "next", &versions, 5),
        Vec::<String>::new()
    );
}

/// The round-15 cases: a version past every release gets as many
/// suggestions as one in the middle (npm `99.0.0`), and PyPI orders a dev
/// release before its final (PEP 440), never between two finals.
#[test]
fn every_missing_version_gets_one_call_of_suggestions() {
    let vite = [
        "8.3.4", "8.3.5", "8.3.6", "8.3.7", "8.3.8", "5.4.20", "5.4.21",
    ];
    assert_eq!(
        suggest(ArtifactType::Npm, "99.0.0", &vite, 5),
        ["8.3.8", "8.3.7", "8.3.6", "8.3.5", "8.3.4"]
    );
    let requests = [
        "2.33.0",
        "2.33.1",
        "2.34.0.dev1",
        "2.34.0",
        "2.34.1",
        "2.34.2",
    ];
    // Final releases first: the dev release fills only a call finals cannot.
    assert_eq!(
        suggest(ArtifactType::Pypi, "2.99.1", &requests, 5),
        ["2.34.2", "2.34.1", "2.34.0", "2.33.1", "2.33.0"]
    );
    assert_eq!(
        suggest(ArtifactType::Pypi, "2.99.1", &requests, 6),
        [
            "2.34.2",
            "2.34.1",
            "2.34.0",
            "2.34.0.dev1",
            "2.33.1",
            "2.33.0"
        ]
    );
    // A prerelease asked for: prereleases rank with the finals.
    assert_eq!(
        suggest(ArtifactType::Pypi, "2.34.0.dev2", &requests, 2),
        ["2.34.0", "2.34.0.dev1"]
    );
}

#[test]
fn fewer_versions_than_one_call_say_so() {
    let published = vec!["1.0.0".to_owned(), "1.1.0".to_owned()];
    let error = version_not_found(ArtifactType::Crates, "pkg", "9.0.0", &published);
    let (picked, fewer) = suggested(ArtifactType::Crates, &error, 5);
    assert_eq!(picked, ["1.1.0", "1.0.0"]);
    assert!(fewer.is_some_and(|hint| hint.contains('2')));
    let (_, full) = suggested(ArtifactType::Crates, &error, 2);
    assert_eq!(full, None);
}

#[test]
fn each_registry_orders_by_its_own_rules() {
    // Maven: qualifiers before the release, a service pack after it.
    assert_eq!(
        suggest(
            ArtifactType::Maven,
            "2.1",
            &["2.0-rc1", "2.0", "2.0-sp1", "2.0.1", "1.9"],
            5
        ),
        ["2.0.1", "2.0-sp1", "2.0", "2.0-rc1", "1.9"]
    );
    // RubyGems: a lettered segment is a prerelease of its release.
    assert_eq!(
        suggest(
            ArtifactType::Rubygems,
            "3.0.0",
            &["2.9.0", "3.0.0.rc1", "2.10.0"],
            3
        ),
        ["3.0.0.rc1", "2.10.0", "2.9.0"]
    );
    // NuGet: four release numbers.
    assert_eq!(
        suggest(
            ArtifactType::Nuget,
            "1.0.0.9",
            &["1.0.0.10", "1.0.0.2", "1.0.0"],
            3
        ),
        ["1.0.0.10", "1.0.0.2", "1.0.0"]
    );
}
