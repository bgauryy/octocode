use super::*;

/// Facts extracted under another schema build another graph, so a
/// snapshot of an unchanged tree from that schema is not reused.
#[test]
fn each_representation_version_changes_the_build_identity() {
    let options = IngestOptions::default();
    let current = build_identity(&options, None, "registry", None);
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
            identity_of(versions, &options, None, "registry", patterns, None),
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
        identity_of(Versions::CURRENT, &options, None, "registry", own, None),
        build_identity(&options, None, "registry", None)
    );
    assert_ne!(
        identity_of(
            Versions::CURRENT,
            &options,
            None,
            "registry",
            "other list",
            None
        ),
        build_identity(&options, None, "registry", None)
    );
}

/// Toggling SCIP consent, or changing any input its cache key hashes, never
/// reuses a snapshot built under the other state.
#[test]
fn the_scip_key_and_consent_change_the_build_identity() {
    let options = IngestOptions::default();
    let off = build_identity(&options, None, "registry", None);
    let first = build_identity(&options, None, "registry", Some("key-a"));
    let second = build_identity(&options, None, "registry", Some("key-b"));
    assert_ne!(off, first);
    assert_ne!(first, second);
    assert_eq!(off, build_identity(&options, None, "registry", None));
}
