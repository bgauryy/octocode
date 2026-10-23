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
    let resolve = |range| npm_resolve(range, versions, Some("4.6.5"));
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
fn cargo_and_pypi_ranges() {
    assert_eq!(
        cargo_resolve("^1.0.100", ["1.0.99", "1.0.100", "1.0.228", "2.0.0"]).as_deref(),
        Some("1.0.228")
    );
    let pypi = ["2.30.0", "2.31.0", "2.32.3", "3.0.0rc1", "1.2"];
    assert_eq!(pypi_resolve(">=2.31,<3", pypi).as_deref(), Some("2.32.3"));
    assert_eq!(pypi_resolve("~=2.31", pypi).as_deref(), Some("2.32.3"));
    assert_eq!(pypi_resolve("~=2.31.0", pypi).as_deref(), Some("2.31.0"));
    assert_eq!(pypi_resolve("==2.30.*", pypi).as_deref(), Some("2.30.0"));
    assert_eq!(
        pypi_resolve("!=2.32.3,>=2", pypi).as_deref(),
        Some("2.31.0")
    );
    assert_eq!(pypi_resolve("nonsense", pypi), None);
}

#[test]
fn nearest_versions_share_the_longest_prefix() {
    let versions = ["1.0.99", "1.0.100", "1.0.101", "1.1.0", "2.0.0"];
    assert_eq!(
        nearest("1.0.105", versions),
        ["1.0.101", "1.0.100", "1.0.99"]
    );
    // No shared prefix: the final releases on either side of it.
    assert_eq!(nearest("3.0.0", versions), ["2.0.0", "1.1.0", "1.0.101"]);
    assert_eq!(
        nearest(
            "5.0.0",
            ["0.4.0", "4.9.1", "6.0.0", "6.1.0", "6.2.0", "7.0.0-rc.1"]
        ),
        ["6.1.0", "6.0.0", "4.9.1", "0.4.0"]
    );
    assert_eq!(nearest("next", versions), Vec::<String>::new());
}
