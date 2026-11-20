//! Resolve rows as served: which version a request names (a dist-tag
//! spelled like a version, a range past deprecated releases, a Cargo version
//! listed with build metadata), the version a missing one is named by, and
//! where a `/vN` Go module's files sit.
use super::*;
use crate::artifact::test_http::{KnownTags, RouteHttp, test_budget};
use octocode_test_support::json_fixture;

/// The served data of `query` against the registry double `http` (and GitHub
/// double `tags`), as [`execute`] serves it: the row, or the error with its
/// repair lead.
async fn served(
    query: Value,
    http: &RouteHttp,
    tags: Option<&KnownTags>,
) -> Result<Value, ArtifactError> {
    let query = typed(&query);
    let budget = test_budget();
    let reused = ReusedSince::new();
    let registry = npm_registry(&query, &NO_ENV)?;
    serve(
        &query,
        &ArtifactProviderContext {
            http,
            budget: &budget,
            npm_registry: registry.as_ref(),
            allow_private_registry: false,
            answers: None,
            reused: &reused,
            tags: tags.map(|tags| tags as &dyn ReleaseTags),
            share: None,
        },
    )
    .await
}

fn npm(version: &str) -> Value {
    json!({"operation": "resolve", "ecosystem": "npm", "packageName": "pkg", "version": version})
}

/// The versions the repair lead of a missing version reads.
fn repair_versions(error: &ArtifactError) -> Vec<String> {
    error
        .calls
        .as_ref()
        .and_then(|calls| calls["repair"]["input"]["queries"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|row| row["version"].as_str().map(str::to_owned))
        .collect()
}

/// An npm registry double: the abbreviated packument `packument`, and each
/// of `manifests` (`(path segment, version, deprecation)`) at
/// `/pkg/<segment>`.
fn npm_registry_double(packument: Value, manifests: &[(&str, &str, Option<&str>)]) -> RouteHttp {
    manifests.iter().fold(
        RouteHttp::new().json("/pkg", packument),
        |http, (segment, version, deprecated)| {
            let mut manifest = json!({"name": "pkg", "version": version});
            if let Some(message) = deprecated {
                manifest["deprecated"] = json!(message);
            }
            http.json(&format!("/pkg/{segment}"), manifest)
        },
    )
}

/// A dist-tag spelled like a version (`v12-lts`) is the tag, as npm reads
/// it: the row is the version it names. `v12` still reads as the 12.x
/// range, and a `v`-spelled tag no version of that range matches (`v9`
/// naming a prerelease) reads its tag.
#[tokio::test]
async fn a_dist_tag_spelled_like_a_version_reads_that_tag() {
    let packument = json_fixture(
        r#"{
            "dist-tags": {"latest": "17.0.0", "v12-lts": "12.2.17", "v9": "9.0.0-legacy.1"},
            "versions": {"9.0.0-legacy.1": {}, "12.2.16": {}, "12.2.17": {}, "17.0.0": {}}
        }"#,
    );
    let http = npm_registry_double(
        packument,
        &[
            ("v12-lts", "12.2.17", None),
            ("12.2.17", "12.2.17", None),
            ("9.0.0-legacy.1", "9.0.0-legacy.1", None),
        ],
    );
    for (asked, version) in [
        ("v12-lts", "12.2.17"),
        ("v12", "12.2.17"),
        ("v9", "9.0.0-legacy.1"),
    ] {
        let data = served(npm(asked), &http, None)
            .await
            .unwrap_or_else(|error| panic!("{asked}: {}", error.message));
        assert_eq!(data["artifacts"][0]["version"], version, "{asked}: {data}");
    }
}

/// A range resolves as `npm install` picks: past a deprecated `latest` to the
/// highest match that is not deprecated; when every match is deprecated, the
/// highest one, whose row states the deprecation.
#[tokio::test]
async fn a_range_passes_over_deprecated_versions_as_npm_install_does() {
    let http = npm_registry_double(
        json_fixture(
            r#"{
                "dist-tags": {"latest": "1.2.0"},
                "versions": {
                    "1.0.0": {"name": "pkg"},
                    "1.1.0": {"name": "pkg", "deprecated": ""},
                    "1.2.0": {"name": "pkg", "deprecated": "broken build"}
                }
            }"#,
        ),
        &[
            ("1.1.0", "1.1.0", None),
            ("1.2.0", "1.2.0", Some("broken build")),
        ],
    );
    let data = served(npm("^1.0.0"), &http, None).await.expect("range");
    assert_eq!(data["artifacts"][0]["version"], "1.1.0", "{data}");
    assert!(data["artifacts"][0].get("deprecated").is_none(), "{data}");

    let every = npm_registry_double(
        json_fixture(
            r#"{
                "dist-tags": {"latest": "1.2.0"},
                "versions": {
                    "1.1.0": {"deprecated": "old"},
                    "1.2.0": {"deprecated": "broken build"}
                }
            }"#,
        ),
        &[("1.2.0", "1.2.0", Some("broken build"))],
    );
    let data = served(npm("^1.0.0"), &every, None).await.expect("range");
    let row = &data["artifacts"][0];
    assert_eq!(
        (&row["version"], &row["deprecated"]),
        (&json!("1.2.0"), &json!("broken build")),
        "{data}"
    );
}

/// A missing version is named as the request typed it (`v99.0.0`,
/// `=0.1.99`), and its repair reads the same nearest versions as the bare
/// spelling's.
#[tokio::test]
async fn a_missing_version_is_named_as_typed() {
    let npm_http = npm_registry_double(
        json_fixture(
            r#"{
                "dist-tags": {"latest": "1.2.0"},
                "versions": {"1.0.0": {}, "1.1.0": {}, "1.2.0": {}}
            }"#,
        ),
        &[],
    );
    let crates_http = RouteHttp::new().json(
        "/api/v1/crates/time",
        json_fixture(
            r#"{
                "crate": {"name": "time", "max_stable_version": "0.1.45"},
                "versions": [{"num": "0.1.45"}, {"num": "0.1.44"}, {"num": "0.1.43"}]
            }"#,
        ),
    );
    let go_http = RouteHttp::new().json(
        "/v1/versions/github.com/o/r/v2",
        json_fixture(r#"{"items": [{"version": "v2.3.0"}, {"version": "v2.2.0"}]}"#),
    );
    let crates = |version: &str| json!({"operation": "resolve", "ecosystem": "crates", "packageName": "time", "version": version});
    let go = |version: &str| json!({"operation": "resolve", "ecosystem": "go", "packageName": "github.com/o/r/v2", "version": version});
    for (typed, bare, http) in [
        (npm("v99.0.0"), npm("99.0.0"), &npm_http),
        (crates("=0.1.99"), crates("0.1.99"), &crates_http),
        (go("v2.99.0"), go("2.99.0"), &go_http),
    ] {
        let asked = typed["version"].as_str().unwrap_or_default().to_owned();
        let missing = served(typed, http, None).await.expect_err("missing");
        assert!(
            missing.message.contains(&format!("\"{asked}\"")),
            "{asked}: {}",
            missing.message
        );
        let plain = served(bare, http, None).await.expect_err("missing");
        let nearest = repair_versions(&missing);
        assert!(!nearest.is_empty(), "{asked}: {:?}", missing.calls);
        assert_eq!(nearest, repair_versions(&plain), "{asked}");
    }
}

fn wasi(versions: &[&str]) -> RouteHttp {
    RouteHttp::new().json(
        "/api/v1/crates/wasi",
        json!({
            "crate": {"name": "wasi", "max_stable_version": versions[0]},
            "versions": versions.iter().map(|num| json!({"num": num})).collect::<Vec<_>>(),
        }),
    )
}

fn crates_wasi(version: &str) -> Value {
    json!({"operation": "resolve", "ecosystem": "crates", "packageName": "wasi", "version": version})
}

/// A Cargo version crates.io lists only with build metadata is the version a
/// pin without it names (`=0.10.0` matches `0.10.0+wasi-snapshot-preview1`):
/// the row states the registry's whole string. Two such listings, or a
/// request naming other build metadata, stay not found, nearest first.
#[tokio::test]
async fn a_cargo_version_listed_with_build_metadata_is_the_one_a_pin_names() {
    let http = wasi(&[
        "0.11.0+wasi-snapshot-preview1",
        "0.10.0+wasi-snapshot-preview1",
        "0.9.0+wasi-snapshot-preview1",
    ]);
    let data = served(crates_wasi("0.10.0"), &http, None)
        .await
        .expect("listed with build metadata");
    assert_eq!(
        data["artifacts"][0]["version"], "0.10.0+wasi-snapshot-preview1",
        "{data}"
    );
    let other = served(crates_wasi("0.10.0+other"), &http, None)
        .await
        .expect_err("other build metadata");
    assert!(other.missing_version, "{}", other.message);

    let two = wasi(&["0.10.0+a", "0.10.0+b", "0.9.0"]);
    let error = served(crates_wasi("0.10.0"), &two, None)
        .await
        .expect_err("two listings");
    let mut nearest = repair_versions(&error);
    nearest.truncate(2);
    nearest.sort();
    assert_eq!(nearest, ["0.10.0+a", "0.10.0+b"], "{:?}", error.calls);
}

const SHA: &str = "825555f65f6e46e06a89d7cad2bc738adaf9344a";

fn go_v2() -> RouteHttp {
    RouteHttp::new().json(
        "/v1/module/github.com/o/r/v2?version=v2.3.0",
        json!({"path": "github.com/o/r/v2", "version": "v2.3.0", "repoUrl": "https://github.com/o/r"}),
    )
}

fn go_resolve(module: &str, version: &str) -> Value {
    json!({"operation": "resolve", "ecosystem": "go", "packageName": module,
        "version": version, "view": "summary"})
}

/// A `/v2` module whose files sit in the repository's `v2` directory at the
/// release's commit is read there; one whose `go.mod` is at the tag prefix (a
/// major branch) keeps it. The ref is the release tag's commit either way.
#[tokio::test]
async fn a_major_version_module_is_read_in_its_vn_directory_when_it_has_one() {
    let in_v2 = KnownTags::new(vec!["o/r@v2.3.0=825555f65f6e46e06a89d7cad2bc738adaf9344a"])
        .with_tree(SHA, vec!["go.mod", "v2/go.mod", "v2/call.go"]);
    let data = served(
        go_resolve("github.com/o/r/v2", "v2.3.0"),
        &go_v2(),
        Some(&in_v2),
    )
    .await
    .expect("module");
    let row = &data["artifacts"][0];
    assert_eq!(
        (&row["ref"], &row["repositoryDirectory"]),
        (&json!(SHA), &json!("v2")),
        "{data}"
    );

    let at_root = KnownTags::new(vec!["o/r@v2.3.0=825555f65f6e46e06a89d7cad2bc738adaf9344a"])
        .with_tree(SHA, vec!["go.mod", "call.go"]);
    let data = served(
        go_resolve("github.com/o/r/v2", "v2.3.0"),
        &go_v2(),
        Some(&at_root),
    )
    .await
    .expect("module");
    let row = &data["artifacts"][0];
    assert_eq!(row["ref"], SHA, "{data}");
    assert!(row.get("repositoryDirectory").is_none(), "{data}");
    assert!(
        !data["warnings"].to_string().contains("repositoryDirectory"),
        "{data}"
    );
}

/// A tree GitHub could not read leaves the directory unknown, and the
/// response says it is one of the two.
#[tokio::test]
async fn an_unread_tree_leaves_a_major_version_directory_unknown_and_says_so() {
    let unreadable = KnownTags::always(None);
    let data = served(
        go_resolve("github.com/o/r/v2", "v2.3.0"),
        &go_v2(),
        Some(&unreadable),
    )
    .await
    .expect("module");
    assert!(
        data["artifacts"][0].get("repositoryDirectory").is_none(),
        "{data}"
    );
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("repositoryDirectory is unknown, v2 or the repository root"),
        "{data}"
    );
}

/// A vanity `/vN` module whose proxy record names the tag prefix (`client`)
/// is read in `client/v3` when that directory holds its `go.mod`.
#[tokio::test]
async fn a_vanity_major_version_module_is_read_below_its_tag_prefix() {
    let http = RouteHttp::new()
        .json(
            "/v1/module/go.etcd.io/etcd/client/v3?version=v3.5.0",
            json!({"path": "go.etcd.io/etcd/client/v3", "version": "v3.5.0",
                "repoUrl": "https://github.com/etcd-io/etcd"}),
        )
        .json(
            "/go.etcd.io/etcd/client/v3/@v/v3.5.0.info",
            json!({"Version": "v3.5.0", "Origin": {"VCS": "git",
                "URL": "https://github.com/etcd-io/etcd", "Subdir": "client",
                "Hash": SHA}}),
        );
    let tags = KnownTags::new(vec![]).with_tree(SHA, vec!["go.mod", "client/v3/go.mod"]);
    let data = served(
        go_resolve("go.etcd.io/etcd/client/v3", "v3.5.0"),
        &http,
        Some(&tags),
    )
    .await
    .expect("module");
    let row = &data["artifacts"][0];
    assert_eq!(
        (&row["ref"], &row["repositoryDirectory"]),
        (&json!(SHA), &json!("client/v3")),
        "{data}"
    );
}

/// A resolve summary states the name and version only when they are news: a
/// pinned version and the name asked are the request's own; a range or a
/// default resolve names the version it read.
#[tokio::test]
async fn a_resolve_summary_drops_the_version_the_request_pinned() {
    let http = npm_registry_double(
        json_fixture(
            r#"{"dist-tags": {"latest": "1.2.0"}, "versions": {"1.1.0": {}, "1.2.0": {}}}"#,
        ),
        &[("1.2.0", "1.2.0", None), ("latest", "1.2.0", None)],
    );
    let summary = |version: Option<&str>| {
        let mut query = json!({"operation": "resolve", "ecosystem": "npm", "packageName": "pkg",
            "view": "summary"});
        if let Some(version) = version {
            query["version"] = json!(version);
        }
        query
    };
    let pinned = served(summary(Some("1.2.0")), &http, None)
        .await
        .expect("pinned");
    assert!(pinned["artifacts"][0].get("version").is_none(), "{pinned}");
    assert!(
        pinned["artifacts"][0].get("packageName").is_none(),
        "{pinned}"
    );
    for asked in [None, Some("^1.0.0"), Some("v1.2.0")] {
        let data = served(summary(asked), &http, None).await.expect("resolved");
        assert_eq!(
            data["artifacts"][0]["version"], "1.2.0",
            "{asked:?}: {data}"
        );
    }
}

/// npm's bulk downloads path for `name` and every name one edit away, as one
/// request asks them.
fn npm_counts_path(name: &str) -> String {
    let names = std::iter::once(name.to_owned())
        .chain(crate::artifact::lookalike::edits(
            name,
            &['-', '_', '.'],
            str::to_owned,
        ))
        .collect::<Vec<_>>();
    format!("/downloads/point/last-week/{}", names.join(","))
}

/// A found name with a far more downloaded name one edit away (the
/// contract's ratio) warns that it may be a typosquat and leads to that
/// name; one that is not far more downloaded does neither.
#[tokio::test]
async fn a_far_more_downloaded_name_one_edit_away_is_flagged() {
    let ratio = octocode_runtime_base::contracts::security::security_contract()
        .expect("contract")
        .policy
        .lookalike_downloads_ratio;
    let row = |name: &str| json!({"operation": "resolve", "ecosystem": "npm", "packageName": name});
    let registry = |other: u64| {
        RouteHttp::new()
            .json(
                "/expresss/latest",
                json!({"name": "expresss", "version": "0.0.0"}),
            )
            .json(
                &npm_counts_path("expresss"),
                json!({
                    "expresss": {"downloads": 743},
                    "express": {"downloads": other},
                    "exprss": {"downloads": 57}
                }),
            )
    };
    let flagged = served(row("expresss"), &registry(743 * ratio), None)
        .await
        .expect("row");
    let warned = flagged["warnings"].to_string();
    assert!(
        warned.contains("express, one edit away") && warned.contains("lookalikeDownloadsRatio"),
        "{flagged}"
    );
    assert_eq!(
        flagged["related"]
            .as_array()
            .expect("related")
            .iter()
            .find(|call| call["tool"] == "artifactSearch")
            .map(|call| call["input"]["queries"][0]["packageName"].clone()),
        Some(json!("express")),
        "{flagged}"
    );
    let near = served(row("expresss"), &registry(743 * ratio - 1), None)
        .await
        .expect("row");
    assert!(!near.to_string().contains("one edit away"), "{near}");
}

/// A name the registry lacks, with names one edit away it has, leads to
/// those names, most downloaded first, and its error names them.
#[tokio::test]
async fn a_missing_name_leads_to_the_names_one_edit_away() {
    let http = RouteHttp::new().json(
        &format!(
            "/api/v1/crates?per_page=100&{}",
            std::iter::once("serdee".to_owned())
                .chain(crate::artifact::lookalike::edits(
                    "serdee",
                    &['-', '_'],
                    |name| { name.to_ascii_lowercase().replace('_', "-") }
                ))
                .map(|name| format!("ids%5B%5D={name}"))
                .collect::<Vec<_>>()
                .join("&")
        ),
        json!({"crates": [
            {"name": "sedree", "recent_downloads": 2},
            {"name": "serde", "recent_downloads": 447_414_642u64}
        ]}),
    );
    let error = served(
        json!({"operation": "resolve", "ecosystem": "crates", "packageName": "serdee"}),
        &http,
        None,
    )
    .await
    .expect_err("missing");
    assert!(
        error.message.contains("serde (447414642"),
        "{}",
        error.message
    );
    let names = error.calls.as_ref().expect("repair")["repair"]["input"]["queries"]
        .as_array()
        .expect("queries")
        .iter()
        .map(|row| row["packageName"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, [json!("serde"), json!("sedree")]);
}

/// An npm name spelled with capitals the registry lacks leads to its
/// lowercase spelling first when that is the most downloaded, never to a
/// little-downloaded name one other edit away.
#[tokio::test]
async fn a_missing_capitalized_npm_name_leads_to_its_lowercase_spelling() {
    let http = RouteHttp::new().json(
        &npm_counts_path("React"),
        json!({
            "React": null,
            "react": {"downloads": 50_000_000u64},
            "eact": {"downloads": 20}
        }),
    );
    let error = served(
        json!({"operation": "resolve", "ecosystem": "npm", "packageName": "React"}),
        &http,
        None,
    )
    .await
    .expect_err("missing");
    let names = error.calls.as_ref().expect("repair")["repair"]["input"]["queries"]
        .as_array()
        .expect("queries")
        .iter()
        .map(|row| row["packageName"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names.first(), Some(&json!("react")), "{}", error.message);
    assert!(error.message.contains("react ("), "{}", error.message);
    // npm was asked the lowercase spelling, not handed it.
    let seen = http.seen();
    assert!(
        seen.iter().any(|request| request
            .url
            .path()
            .rsplit('/')
            .next()
            .is_some_and(|names| names.split(',').any(|name| name == "react"))),
        "{seen:?}"
    );
}

/// A scoped npm name the registry lacks leads to its scope's packages,
/// never to a search of the whole name's words.
#[tokio::test]
async fn a_missing_scoped_name_searches_its_scope() {
    let error = served(
        json!({"operation": "resolve", "ecosystem": "npm", "packageName": "@octocodeai/definitely-not-a-real-pkg-zz9"}),
        &RouteHttp::new(),
        None,
    )
    .await
    .expect_err("missing");
    let lead = &error.calls.as_ref().expect("lead")["broadenSearch"]["input"]["queries"][0];
    assert_eq!(lead["keywords"], json!(["@octocodeai"]), "{lead}");
}
