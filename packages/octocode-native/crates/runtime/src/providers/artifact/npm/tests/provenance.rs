//! Provenance attestations and path-scoped registry credentials.
use super::{
    COMMIT, INSTALL, JSON, TARBALL, bundle, lookup, manifest, npmrc_registry, requests, routes,
    sha512,
};
use crate::providers::artifact::ArtifactItem;
use crate::providers::artifact::http::RegistryClient;
use crate::providers::artifact::npm::exact;
use crate::providers::artifact::npm::provenance::attested_commit;
use crate::providers::artifact::npm::search::search;
use crate::providers::artifact::query::artifact_query;
use crate::providers::artifact::test_http::test_budget;
use octocode_contract_types::ArtifactType;
use serde_json::json;

#[tokio::test]
async fn npm_provenance_pins_the_attested_commit_only_when_bound() {
    let good = routes(vec![
        ("/zod/4.6.5", JSON, manifest("4.6.5", true)),
        (
            "/-/npm/v1/attestations/zod@4.6.5",
            JSON,
            bundle(
                "4.6.5",
                "https://github.com/colinhacks/zod",
                &sha512(TARBALL),
            ),
        ),
    ]);
    let page = lookup(
        &good,
        json!({"operation":"resolve","packageName": "zod", "version": "4.6.5"}),
    )
    .await
    .expect("attested");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some(COMMIT));
    assert!(item.source_attested);

    for (repo, digest) in [
        ("https://github.com/evil/zod", sha512(TARBALL)),
        (
            "https://github.com/colinhacks/zod",
            sha512(b"other tarball"),
        ),
    ] {
        let spoofed = routes(vec![
            ("/zod/4.6.5", JSON, manifest("4.6.5", true)),
            (
                "/-/npm/v1/attestations/zod@4.6.5",
                JSON,
                bundle("4.6.5", repo, &digest),
            ),
        ]);
        let page = lookup(
            &spoofed,
            json!({"operation":"resolve","packageName": "zod", "version": "4.6.5"}),
        )
        .await
        .expect("unbound attestation");
        let item = &page.artifacts[0];
        assert!(!item.source_attested, "{repo}");
        assert_eq!(
            item.source_ref.as_deref(),
            Some("aaaaaaa1"),
            "gitHead lead stays"
        );
    }
}

#[tokio::test]
async fn attestation_requests_keep_fake_credentials_on_the_registry_origin() {
    for (url, allowed) in [
        ("https://registry.npmjs.org:444/attestation", false),
        ("https://fake:secret@registry.npmjs.org/attestation", false),
        ("https://registry.npmjs.org:443/attestation", true),
    ] {
        let http = routes(vec![(
            "/attestation",
            JSON,
            bundle(
                "4.6.5",
                "https://github.com/colinhacks/zod",
                &sha512(TARBALL),
            ),
        )]);
        let budget = test_budget();
        let client = RegistryClient::uncached(&http, &budget);
        let registry = npmrc_registry(
            "https://registry.npmjs.org/",
            "//registry.npmjs.org/:_authToken=fake-test-token",
        );
        let mut item =
            ArtifactItem::new(ArtifactType::Npm, "zod".into(), registry.base.to_string());
        item.repository = Some("https://github.com/colinhacks/zod".into());
        let mut row = manifest("4.6.5", true);
        row["dist"]["attestations"]["url"] = json!(url);
        let commit = attested_commit(
            &item,
            "4.6.5",
            row.as_object().expect("manifest"),
            &registry,
            &client,
            None,
        )
        .await;
        let requests = requests(&http);
        if allowed {
            assert_eq!(commit.as_deref(), Some(COMMIT));
            assert_eq!(requests.len(), 1);
            assert_eq!(
                requests[0],
                ("https://registry.npmjs.org/attestation".into(), true)
            );
        } else {
            assert!(
                requests.is_empty(),
                "off-origin or userinfo URL reached authenticated transport: {url}"
            );
            assert!(commit.is_none());
        }
    }
}

#[tokio::test]
async fn npm_path_scoped_credentials_do_not_reach_sibling_attestations() {
    for (scope, target, authenticated) in [
        ("/api/npm/", "/api/npm/attestation", true),
        ("/api/npm/", "/other-service/attestation", false),
        ("/api/npm/", "/api/npm-extra/attestation", false),
        ("/", "/other-service/attestation", true),
    ] {
        let http = routes(vec![(
            target,
            JSON,
            bundle(
                "4.6.5",
                "https://github.com/colinhacks/zod",
                &sha512(TARBALL),
            ),
        )]);
        let budget = test_budget();
        let client = RegistryClient::uncached(&http, &budget);
        let registry = npmrc_registry(
            "https://registry.npmjs.org/api/npm/",
            &format!("//registry.npmjs.org{scope}:_authToken=fake-path-test-token"),
        );
        assert!(registry.authorization.is_some());
        let mut item =
            ArtifactItem::new(ArtifactType::Npm, "zod".into(), registry.base.to_string());
        item.repository = Some("https://github.com/colinhacks/zod".into());
        let mut row = manifest("4.6.5", true);
        let url = format!("https://registry.npmjs.org{target}");
        row["dist"]["attestations"]["url"] = json!(url);
        let commit = attested_commit(
            &item,
            "4.6.5",
            row.as_object().expect("manifest"),
            &registry,
            &client,
            None,
        )
        .await;
        assert_eq!(
            commit.as_deref(),
            Some(COMMIT),
            "anonymous optional attestations remain usable"
        );
        assert_eq!(
            requests(&http),
            [(url, authenticated)],
            "credential scope {scope}, target {target}"
        );
    }
}

#[tokio::test]
async fn npm_path_scoped_credentials_authorize_metadata_requests() {
    let http = routes(vec![
        (
            "/api/npm/zod",
            INSTALL,
            json!({"name":"zod","dist-tags":{"latest":"4.6.5"},"versions":{"4.6.5":{}}}),
        ),
        ("/api/npm/zod/4.6.5", JSON, manifest("4.6.5", false)),
        (
            "/api/npm/-/v1/search",
            JSON,
            json!({"objects":[],"total":0}),
        ),
    ]);
    let budget = test_budget();
    let client = RegistryClient::uncached(&http, &budget);
    let registry = npmrc_registry(
        "https://registry.npmjs.org/api/npm/",
        "//registry.npmjs.org/api/npm/:_authToken=fake-metadata-token",
    );
    exact("zod", Some("^4"), &registry, &client, None)
        .await
        .expect("packument and manifest");
    let query = artifact_query(
        json!({"operation":"search","ecosystem":"npm","keywords":["schema"]}),
        None,
    );
    search(&query, &Default::default(), &registry, &client, None)
        .await
        .expect("discovery");
    let requests = requests(&http);
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|(url, auth)| url.starts_with("https://registry.npmjs.org/api/npm/") && *auth),
        "{requests:?}"
    );
}
