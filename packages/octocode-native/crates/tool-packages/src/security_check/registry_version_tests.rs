//! The version a package row checks is the one the registry resolves the
//! request to: a dist-tag spelled like a version, a Cargo version listed with
//! build metadata.
use super::*;

/// A version the request names that the registry answers is checked, and
/// OSV is asked about the registry's version.
async fn checked(http: &RouteHttp, ecosystem: &str, name: &str, version: &str) -> Value {
    let data = answer(
        http,
        json!({"operation": "package", "ecosystem": ecosystem, "packageName": name,
            "version": version}),
    )
    .await
    .unwrap_or_else(|error| panic!("{name} {version}: {}", error.message));
    assert!(data.get("errorCode").is_none(), "{data}");
    assert!(data.get("versionEvaluated").is_none(), "{data}");
    data
}

/// npm `v12-lts` is a dist-tag: the row checks the version it names.
#[tokio::test]
async fn a_dist_tag_spelled_like_a_version_checks_the_version_it_names() {
    let packument = json_fixture(
        r#"{
            "dist-tags": {"latest": "17.0.0", "v12-lts": "12.2.17"},
            "versions": {"12.2.17": {}, "17.0.0": {}}
        }"#,
    );
    let http = RouteHttp::new()
        .json("/pkg", packument)
        .json("/pkg/v12-lts", json!({"name": "pkg", "version": "12.2.17"}))
        .osv_pages(vec![(200, json!({}))]);
    checked(&http, "npm", "pkg", "v12-lts").await;
    assert_eq!(http.osv_queries()[0]["version"], "12.2.17");
}

/// The pin a Cargo lockfile holds without build metadata (`wasi 0.10.0`)
/// names the release crates.io lists with it.
#[tokio::test]
async fn a_cargo_pin_without_build_metadata_checks_the_listed_release() {
    let http = RouteHttp::new()
        .json(
            "/api/v1/crates/wasi",
            json_fixture(
                r#"{
                    "crate": {"name": "wasi", "max_stable_version": "0.11.0+wasi-snapshot-preview1"},
                    "versions": [
                        {"num": "0.11.0+wasi-snapshot-preview1"},
                        {"num": "0.10.0+wasi-snapshot-preview1"}
                    ]
                }"#,
            ),
        )
        .osv_pages(vec![(200, json!({}))]);
    checked(&http, "crates", "wasi", "0.10.0").await;
    assert_eq!(
        http.osv_queries()[0]["version"],
        "0.10.0+wasi-snapshot-preview1"
    );
}
