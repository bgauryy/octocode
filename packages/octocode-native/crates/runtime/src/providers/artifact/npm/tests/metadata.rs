use super::*;

/// `dependencies` lists the required ones as `name@range`: npm repeats
/// each optional dependency under `dependencies`, and those are listed
/// in `optionalDependencies` instead; a version without optional ones
/// shows no such list.
#[tokio::test]
async fn npm_optional_dependencies_are_listed_apart_from_required_ones() {
    let mut row = manifest("3.22.0", false);
    row["dependencies"] = json!({"a": "^1.0.0", "@next/swc-darwin-arm64": "15.0.0"});
    row["optionalDependencies"] = json!({"@next/swc-darwin-arm64": "15.0.0"});
    let http = routes(vec![]).json_for("/zod/3.22.0", JSON, row);
    let page = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "3.22.0"}),
    )
    .await
    .expect("exact");
    assert_eq!(
        page.artifacts[0].dependencies,
        Some(vec!["a@^1.0.0".to_owned()])
    );
    assert_eq!(
        page.artifacts[0].optional_dependencies,
        Some(vec!["@next/swc-darwin-arm64@15.0.0".to_owned()])
    );
    let mut bare = manifest("3.22.0", false);
    bare["dependencies"] = json!({"a": "^1.0.0"});
    let http = routes(vec![]).json_for("/zod/3.22.0", JSON, bare);
    let page = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "3.22.0"}),
    )
    .await
    .expect("exact");
    assert_eq!(page.artifacts[0].optional_dependencies, None);
}

/// The object form's `directory` is the registry's word: the link's page never
/// replaces it, and a manifest without one keeps what the link's page gave.
#[tokio::test]
async fn an_npm_repository_directory_is_the_registrys_when_stated_and_never_erased() {
    let tags = super::super::super::test_http::KnownTags::new(vec!["o/r@main"]);
    let resolve = |repository: Value| {
        let tags = &tags;
        async move {
            let mut row = manifest("3.22.0", false);
            row["repository"] = repository;
            let http = routes(vec![]).json_for("/zod/3.22.0", JSON, row);
            let query = super::super::super::query::versioned(
                octocode_contract_types::ArtifactType::Npm,
                "zod",
                "3.22.0",
            );
            super::super::super::test_http::execute(&http, &query, Some(tags))
                .await
                .expect("exact")
                .artifacts
                .remove(0)
        }
    };
    let stated = resolve(json!({"type": "git",
        "url": "git+https://github.com/o/r/tree/main/other",
        "directory": "packages/zod"}))
    .await;
    assert_eq!(stated.repository_directory.as_deref(), Some("packages/zod"));
    assert!(stated.disclosures.is_empty());
    let derived = resolve(json!({"type": "git",
        "url": "git+https://github.com/o/r/tree/main/packages/zod"}))
    .await;
    assert_eq!(
        derived.repository_directory.as_deref(),
        Some("packages/zod")
    );
    assert_eq!(
        derived.repository.as_deref(),
        Some("https://github.com/o/r")
    );
}
