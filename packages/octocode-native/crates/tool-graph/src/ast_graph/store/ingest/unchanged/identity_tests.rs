use super::*;

/// Facts extracted under another schema build another graph, so a
/// snapshot of an unchanged tree from that schema is not reused.
#[test]
fn each_representation_version_changes_the_build_identity() {
    let options = IngestOptions::default();
    let current = build_identity(&options, None, "registry");
    let bumps = [
        Versions {
            format: Versions::CURRENT.format + 1,
            ..Versions::CURRENT
        },
        Versions {
            facts: Versions::CURRENT.facts + 1,
            ..Versions::CURRENT
        },
        Versions {
            analysis: Versions::CURRENT.analysis + 1,
            ..Versions::CURRENT
        },
    ];
    let patterns = octocode_engine::security::patterns_identity();
    for versions in bumps {
        assert_ne!(
            identity_of(versions, &options, None, "registry", patterns),
            current
        );
    }
}

/// A snapshot's redacted spans come from the known-format list: an
/// unchanged tree under another list is not an unchanged snapshot.
#[test]
fn the_known_format_list_changes_the_build_identity() {
    let options = IngestOptions::default();
    let own = octocode_engine::security::patterns_identity();
    assert_eq!(
        identity_of(Versions::CURRENT, &options, None, "registry", own),
        build_identity(&options, None, "registry")
    );
    assert_ne!(
        identity_of(Versions::CURRENT, &options, None, "registry", "other list"),
        build_identity(&options, None, "registry")
    );
}
