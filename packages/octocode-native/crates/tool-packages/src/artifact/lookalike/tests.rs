//! Names one edit away, and what the registry counts of them.
use super::*;
use crate::artifact::test_http::{RouteHttp, test_budget};
use octocode_runtime_base::providers::saved_answers::ReusedSince;
use serde_json::json;

fn same(name: &str) -> String {
    name.to_owned()
}

/// The typosquat shapes are one edit each way: a doubled letter dropped, a
/// swap undone, a separator added or changed.
#[test]
fn the_edits_reach_each_shape_one_edit_away() {
    let npm = ['-', '_', '.'];
    assert!(edits("expresss", &npm, same).contains(&"express".to_owned()));
    assert!(edits("loadsh", &npm, same).contains(&"lodash".to_owned()));
    assert!(edits("crossenv", &npm, same).contains(&"cross-env".to_owned()));
    assert!(edits("cross_env", &npm, same).contains(&"cross-env".to_owned()));
    assert!(edits("serdee", &['-', '_'], crate_identity).contains(&"serde".to_owned()));
    for name in ["expresss", "a-b", "x"] {
        let found = edits(name, &npm, same);
        assert!(!found.contains(&name.to_owned()), "{name}: {found:?}");
        let mut unique = found.clone();
        unique.dedup();
        assert_eq!(unique.len(), found.len(), "{found:?}");
        // Every edit starts and ends with a letter or digit, as names do.
        assert!(
            found.iter().all(
                |edit| edit
                    .chars()
                    .next()
                    .zip(edit.chars().last())
                    .is_some_and(|(first, last)| first.is_ascii_alphanumeric()
                        && last.is_ascii_alphanumeric())
            ),
            "{found:?}"
        );
    }
    // A crate spelled with `_` for `-` is the same crate: not an edit of it.
    let crates = edits("serde-json", &['-', '_'], crate_identity);
    assert!(!crates.contains(&"serde_json".to_owned()), "{crates:?}");
}

/// A name spelled with capitals is one edit from its lowercase spelling, the
/// only one npm takes for a new name (LEFTOVERS P2); a registry that matches
/// names whatever their case has no such edit.
#[test]
fn capitals_lowercased_are_one_edit() {
    let npm = ['-', '_', '.'];
    for name in ["React", "Lodash.Merge", "JSONStream"] {
        let lower = name.to_lowercase();
        assert!(edits(name, &npm, same).contains(&lower), "{name}");
    }
    assert!(
        !edits("react", &npm, same)
            .iter()
            .any(|edit| edit == "react")
    );
    let crates = edits("Serde", &['-', '_'], crate_identity);
    assert!(!crates.contains(&"serde".to_owned()), "{crates:?}");
}

/// The ratio is the policy's, over the name's own count of one at least.
#[test]
fn a_lookalike_needs_the_ratio_over_the_names_own_count() {
    let near = |own: Option<u64>, other: u64| Near {
        own,
        names: vec![("express".to_owned(), other)],
        measure: "last-week downloads",
    };
    let ratio = 1000;
    assert!(near(Some(743), 743 * ratio).lookalike(ratio).is_some());
    assert!(near(Some(743), 743 * ratio - 1).lookalike(ratio).is_none());
    assert!(near(Some(0), ratio).lookalike(ratio).is_some());
    assert!(near(None, ratio - 1).lookalike(ratio).is_none());
}

async fn read(
    http: &RouteHttp,
    kind: ArtifactType,
    name: &str,
) -> Option<Result<Near, ArtifactError>> {
    let budget = test_budget();
    let reused = ReusedSince::default();
    let context = ArtifactProviderContext {
        http,
        budget: &budget,
        npm_registry: None,
        allow_private_registry: false,
        answers: None,
        reused: &reused,
        tags: None,
        share: None,
    };
    near_names(kind, name, &context).await
}

/// npm's counts come in one map of names for every name asked; a name npm
/// lacks is null and is no near name. The most downloaded comes first.
#[tokio::test]
async fn npm_counts_the_name_and_each_edit_it_has() {
    let http = RouteHttp::always(json!({
        "expresss": {"downloads": 743, "package": "expresss"},
        "express": {"downloads": 207_355_162u64, "package": "express"},
        "exprss": {"downloads": 57, "package": "exprss"},
        "expresz": null
    }));
    let near = read(&http, ArtifactType::Npm, "expresss")
        .await
        .expect("npm is checked")
        .expect("counts");
    assert_eq!(near.own, Some(743));
    assert_eq!(
        near.names,
        [
            ("express".to_owned(), 207_355_162),
            ("exprss".to_owned(), 57)
        ]
    );
    let seen = http.seen();
    // Each request names at most npm's bulk limit, the name first.
    assert!(seen.iter().all(|request| {
        request.url.host_str() == Some("api.npmjs.org")
            && request
                .url
                .path()
                .rsplit('/')
                .next()
                .is_some_and(|names| names.split(',').count() <= NPM_BULK_NAMES)
    }));
    assert!(
        seen[0].url.path().contains("/last-week/expresss,"),
        "{seen:?}"
    );
}

/// A scoped npm name, any other registry and a configured npm registry are
/// not checked, and nothing is asked.
#[tokio::test]
async fn unchecked_registries_ask_nothing() {
    let http = RouteHttp::new();
    for (kind, name) in [
        (ArtifactType::Npm, "@scope/name"),
        (ArtifactType::Pypi, "requests"),
        (ArtifactType::Maven, "g:a"),
    ] {
        assert!(read(&http, kind, name).await.is_none(), "{name}");
    }
    assert!(http.seen().is_empty());
}

/// crates.io answers by identity: the count lands on the name asked, and
/// a crate the request did not name (two edits away) is no near name.
#[tokio::test]
async fn crates_counts_by_identity() {
    let http = RouteHttp::always(json!({"crates": [
        {"id": "serde", "name": "serde", "recent_downloads": 447_414_642u64},
        {"id": "sedree", "name": "sedree", "recent_downloads": 2},
        {"id": "sedre", "name": "sedre", "recent_downloads": 5}
    ], "meta": {"total": 3}}));
    let near = read(&http, ArtifactType::Crates, "serdee")
        .await
        .expect("crates is checked")
        .expect("counts");
    assert_eq!(near.own, None);
    assert_eq!(
        near.names,
        [("serde".to_owned(), 447_414_642), ("sedree".to_owned(), 2)]
    );
    assert_eq!(near.measure, "90-day downloads");
}
