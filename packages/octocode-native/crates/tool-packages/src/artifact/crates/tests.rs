//! crates.io lookups: exact releases, defaults, dependencies, source refs and discovery.
use super::super::query::{exact_query, versioned};
use super::super::test_http::test_budget;
use super::super::test_http::{KnownTags, RouteHttp};
use super::*;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::RequestBudget;
use octocode_test_support::json_fixture;
use serde_json::json;

fn serde_crate_document() -> Value {
    json_fixture(
        r#"{
            "crate": {
                "name": "serde",
                "max_stable_version": "1.0.228",
                "default_version": "1.0.228",
                "description": "serialization",
                "homepage": "https://serde.rs",
                "repository": "https://github.com/serde-rs/serde"
            },
            "versions": [
                {
                    "num": "1.0.228",
                    "created_at": "2025-09-27T00:00:00Z",
                    "yanked": false,
                    "license": "MIT OR Apache-2.0",
                    "rust_version": "1.61"
                },
                {
                    "num": "1.0.200",
                    "created_at": "2024-05-01T00:00:00Z",
                    "yanked": true,
                    "license": "MIT OR Apache-2.0",
                    "rust_version": "1.31"
                },
                {
                    "num": "1.0.100",
                    "created_at": "2019-09-08T01:56:06Z",
                    "yanked": false,
                    "license": "MIT OR Apache-2.0"
                },
                {
                    "num": "1.0.99",
                    "created_at": "2019-08-01T00:00:00Z",
                    "yanked": false
                }
            ]
        }"#,
    )
}

#[tokio::test]
async fn crates_exact_versions_carry_the_packaging_commit_or_release_tag() {
    let info =
        br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":"serde"}"#;
    let http = RouteHttp::routes(vec![("/api/v1/crates/serde", serde_crate_document())]).bytes(
        "/crates/serde/serde-1.0.100.crate",
        super::vcs::test_archive(&[
            ("serde-1.0.100/Cargo.toml", b"[package]", b'0'),
            ("serde-1.0.100/.cargo_vcs_info.json", info, b'0'),
        ]),
    );
    let tags = KnownTags::new(vec![
        "serde-rs/serde@v1.0.228",
        "serde-rs/serde@v1.0.100=b6a77c4413f902523646be0d7f5520631df53ff6",
    ]);
    let b = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let state = ArtifactProviderState::default();
    let pinned = crates(
        &versioned(ArtifactType::Crates, "serde", "1.0.100"),
        &state,
        &client,
    )
    .await
    .expect("pinned")
    .artifacts
    .remove(0);
    assert_eq!(
        pinned.source_ref.as_deref(),
        Some("b6a77c4413f902523646be0d7f5520631df53ff6")
    );
    assert_eq!(pinned.repository_directory.as_deref(), Some("serde"));
    let latest = crates(&exact_query(ArtifactType::Crates, "serde"), &state, &client)
        .await
        .expect("latest")
        .artifacts
        .remove(0);
    assert_eq!(latest.source_ref.as_deref(), Some("commit-of-v1.0.228"));
    assert_eq!(latest.release_tag.as_deref(), Some("v1.0.228"));
    assert!(latest.source_tag, "an upstream tag is checked to exist");
    assert!(
        pinned.source_tag,
        "the release tag names the packaging commit: it is checked"
    );
    assert_eq!(latest.repository_directory, None);
    // A release tag naming another commit leaves cargo's commit the
    // registry's claim, and the row says which commit the tag names.
    let elsewhere = KnownTags::new(vec![
        "serde-rs/serde@v1.0.100=0000000000000000000000000000000000000001",
    ]);
    let client = RegistryClient {
        tags: Some(&elsewhere),
        ..RegistryClient::new(&http, &b)
    };
    let claimed = crates(
        &versioned(ArtifactType::Crates, "serde", "1.0.100"),
        &state,
        &client,
    )
    .await
    .expect("pinned")
    .artifacts
    .remove(0);
    assert!(!claimed.source_tag);
    assert_eq!(
        claimed.tag_differs.as_ref().map(|tag| tag.tag.as_str()),
        Some("v1.0.100")
    );
}

/// A crate row lists its runtime dependencies like the other
/// ecosystems: normal ones, required apart from optional, on both
/// exact-version paths.
#[tokio::test]
async fn crates_rows_list_runtime_dependencies() {
    let dependencies = json_fixture(
        r#"{
            "dependencies": [
                {
                    "crate_id": "a",
                    "kind": "normal",
                    "optional": false,
                    "req": "^1.0"
                },
                {"crate_id": "b", "kind": "normal", "optional": true},
                {"crate_id": "c", "kind": "dev", "optional": false},
                {"crate_id": "d", "kind": "build", "optional": false},
                {"crate_id": "e", "kind": "normal", "optional": false},
                {
                    "crate_id": "f",
                    "kind": "normal",
                    "optional": false,
                    "req": "^2",
                    "target": "cfg(unix)"
                },
                {
                    "crate_id": "f",
                    "kind": "normal",
                    "optional": false,
                    "req": "^2",
                    "target": "cfg(windows)"
                }
            ]
        }"#,
    );
    let document = serde_crate_document();
    let mut metadata = json!({"crate": document["crate"].clone(), "versions": null});
    metadata["crate"]["max_stable_version"] = Value::Null;
    let record = json!({"version": document["versions"][2].clone()});
    let small = RouteHttp::routes(vec![
        ("/api/v1/crates/serde?include=", metadata),
        ("/api/v1/crates/serde/1.0.100", record),
        (
            "/api/v1/crates/serde/1.0.100/dependencies",
            dependencies.clone(),
        ),
    ]);
    let whole = RouteHttp::routes(vec![
        ("/api/v1/crates/serde", document),
        ("/api/v1/crates/serde/1.0.228/dependencies", dependencies),
    ]);
    let b = test_budget();
    let pinned = crate_lookup(
        &small,
        &b,
        &versioned(ArtifactType::Crates, "serde", "1.0.100"),
    )
    .await
    .expect("pinned")
    .artifacts
    .remove(0);
    assert_eq!(pinned.dependencies.as_ref().map(Vec::len), Some(4));
    let latest = crate_lookup(&whole, &b, &exact_query(ArtifactType::Crates, "serde"))
        .await
        .expect("latest")
        .artifacts
        .remove(0);
    // The names ride the row; the list is the one dependency field.
    let row = serde_json::to_value(&latest).expect("row");
    // One dependency per target stays two entries, each with its target.
    assert_eq!(
        row["dependencies"],
        json_fixture(r#"["a ^1.0", "e", "f ^2; cfg(unix)", "f ^2; cfg(windows)"]"#)
    );
    assert_eq!(row["optionalDependencies"], json_fixture(r#"["b"]"#));
}

#[tokio::test]
async fn crates_versions_resolve_from_the_crate_response() {
    let http = RouteHttp::routes(vec![("/api/v1/crates/serde", serde_crate_document())]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let state = ArtifactProviderState::default();
    let lookup = |query: ArtifactSearchQuery| {
        let client = &client;
        let state = &state;
        async move { crates(&query, state, client).await }
    };
    let latest = lookup(exact_query(ArtifactType::Crates, "serde"))
        .await
        .expect("latest");
    assert_eq!(latest.artifacts[0].rust_version.as_deref(), Some("1.61"));
    let range = lookup(versioned(
        ArtifactType::Crates,
        "serde",
        ">=1.0.100, <1.0.228",
    ))
    .await
    .expect("range");
    assert_eq!(
        range.artifacts[0].version.as_deref(),
        Some("1.0.100"),
        "yanked 1.0.200 is skipped"
    );
    // The latest reads the default version's view and the range the
    // crate document; only the resolved version's dependency list is a
    // second read.
    assert!(
        http.targets()
            .iter()
            .filter(|path| path.starts_with("/api/"))
            .all(|path| path == "/api/v1/crates/serde"
                || path == "/api/v1/crates/serde?include=default_version"
                || path.ends_with("/dependencies")),
        "{:?}",
        http.targets()
    );
    // A version the registry does not know reads the crate document for
    // its nearest versions.
    let missing = lookup(versioned(ArtifactType::Crates, "serde", "1.0.101"))
        .await
        .expect_err("missing");
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert!(
        missing.nearest.iter().any(|version| version == "1.0.100"),
        "{missing:?}"
    );
}

async fn crate_lookup(
    http: &RouteHttp,
    budget: &RequestBudget,
    query: &ArtifactSearchQuery,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let client = RegistryClient::new(http, budget);
    crates(query, &ArtifactProviderState::default(), &client).await
}

/// An unpinned or `latest` lookup reads the crate's default version
/// view, never the document listing every version, and returns the row
/// that document gives. The default is crates.io's: the highest stable
/// version, else the highest prerelease, else a yanked one, marked.
#[tokio::test]
async fn an_unpinned_crate_reads_only_its_default_version() {
    let slim = |default: &str, entry: Value| {
        json!({
            "crate": {"name": "serde", "max_stable_version": null, "max_version": "0.0.0",
                "default_version": default, "description": "serialization",
                "homepage": "https://serde.rs", "repository": "https://github.com/serde-rs/serde"},
            "versions": [entry]
        })
    };
    let document = serde_crate_document();
    let stable = slim("1.0.228", document["versions"][0].clone());
    let b = test_budget();
    let whole = RouteHttp::routes(vec![("/api/v1/crates/serde", document.clone())]);
    let from_document = crate_lookup(&whole, &b, &versioned(ArtifactType::Crates, "serde", "*"))
        .await
        .expect("document row")
        .artifacts
        .remove(0);
    for query in [
        exact_query(ArtifactType::Crates, "serde"),
        versioned(ArtifactType::Crates, "serde", "latest"),
    ] {
        let http = RouteHttp::routes(vec![(
            "/api/v1/crates/serde?include=default_version",
            stable.clone(),
        )]);
        let row = crate_lookup(&http, &b, &query)
            .await
            .expect("latest")
            .artifacts
            .remove(0);
        assert_eq!(row, from_document);
        assert!(
            !http
                .targets()
                .iter()
                .any(|target| target == "/api/v1/crates/serde"),
            "{:?}",
            http.targets()
        );
    }
    for (default, entry, yanked) in [
        (
            "2.0.0-rc.1",
            json_fixture(
                r#"{
                    "num": "2.0.0-rc.1",
                    "created_at": "2026-01-02T00:00:00Z",
                    "yanked": false
                }"#,
            ),
            None,
        ),
        (
            "0.3.0",
            json_fixture(
                r#"{
                    "num": "0.3.0",
                    "created_at": "2020-01-02T00:00:00Z",
                    "yanked": true
                }"#,
            ),
            Some(true),
        ),
    ] {
        let http = RouteHttp::routes(vec![(
            "/api/v1/crates/serde?include=default_version",
            slim(default, entry),
        )]);
        let row = crate_lookup(&http, &b, &exact_query(ArtifactType::Crates, "serde"))
            .await
            .expect("default")
            .artifacts
            .remove(0);
        assert_eq!(
            (row.version.as_deref(), row.yanked),
            (Some(default), yanked)
        );
    }
}

/// An exact version reads its own record and the crate's metadata, not
/// the crate document that lists every version, and returns the same
/// row the document would.
#[tokio::test]
async fn crates_exact_versions_read_the_version_record_with_the_same_fields() {
    let document = serde_crate_document();
    let record = |num: &str| {
        json!({"version": document["versions"].as_array().expect("versions").iter()
            .find(|entry| entry["num"] == num).expect("entry")})
    };
    // `include=` leaves the version-derived fields empty, as crates.io does.
    let mut metadata = json!({"crate": document["crate"].clone(), "versions": null});
    metadata["crate"]["max_stable_version"] = Value::Null;
    metadata["crate"]["max_version"] = json!("0.0.0");
    let small = RouteHttp::routes(vec![
        ("/api/v1/crates/serde?include=", metadata),
        ("/api/v1/crates/serde/1.0.100", record("1.0.100")),
        ("/api/v1/crates/serde/1.0.200", record("1.0.200")),
    ]);
    let whole = RouteHttp::routes(vec![("/api/v1/crates/serde", document.clone())]);
    let b = test_budget();
    for (query, version) in [
        (
            versioned(ArtifactType::Crates, "serde", "1.0.100"),
            "1.0.100",
        ),
        (
            versioned(ArtifactType::Crates, "serde", "1.0.200"),
            "1.0.200",
        ),
    ] {
        let item = crate_lookup(&small, &b, &query)
            .await
            .expect("exact")
            .artifacts
            .remove(0);
        assert_eq!(item.version.as_deref(), Some(version));
        let via_document = crate_lookup(&whole, &b, &query)
            .await
            .expect("document")
            .artifacts
            .remove(0);
        assert_eq!(
            serde_json::to_value(&item).expect("item"),
            serde_json::to_value(&via_document).expect("item"),
            "{version}"
        );
    }
    // Registry reads only (dependency lists aside); the release-ref
    // reads are lead sources.
    let mut seen: Vec<String> = small
        .targets()
        .into_iter()
        .filter(|path| path.starts_with("/api/") && !path.ends_with("/dependencies"))
        .collect();
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen,
        [
            "/api/v1/crates/serde/1.0.100",
            "/api/v1/crates/serde/1.0.200",
            "/api/v1/crates/serde?include=",
        ]
    );
}

/// A yanked version's row carries the maintainer's reason when one is stated.
#[tokio::test]
async fn a_yanked_crate_version_keeps_its_yank_message() {
    let mut document = serde_crate_document();
    document["versions"][1]["yank_message"] = json!("miscompiles on arm");
    let http = RouteHttp::routes(vec![("/api/v1/crates/serde", document)]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let query = versioned(ArtifactType::Crates, "serde", "1.0.200");
    let page = crates(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("crates");
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    assert_eq!(row["yanked"], true, "{row}");
    assert_eq!(row["yankedReason"], "miscompiles on arm", "{row}");
    let query = versioned(ArtifactType::Crates, "serde", "1.0.100");
    let page = crates(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("crates");
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    assert!(row.get("yankedReason").is_none(), "{row}");
}

/// A version the registry does not know names which of its nearest versions
/// crates.io marks yanked: published, and no release to move to.
#[tokio::test]
async fn a_missing_version_names_which_nearest_versions_are_yanked() {
    let http = RouteHttp::routes(vec![("/api/v1/crates/serde", serde_crate_document())]);
    let b = test_budget();
    let missing = crate_lookup(
        &http,
        &b,
        &versioned(ArtifactType::Crates, "serde", "1.0.101"),
    )
    .await
    .expect_err("missing");
    assert!(missing.missing_version, "{missing:?}");
    assert_eq!(missing.yanked, ["1.0.200"], "{missing:?}");
    assert!(
        missing.nearest.iter().any(|version| version == "1.0.200"),
        "the yanked release stays among the nearest: {missing:?}"
    );
}

/// A crate's link is its newest release's: an old release packaged before
/// cargo recorded `path_in_vcs` is placed in the tree at its own commit, not
/// at the link's directory. ripgrep's `ignore` 0.4.7 sits in `ignore/` and
/// `grep-searcher` 0.1.6 in `grep-searcher/`, where today's links say
/// `crates/ignore` and `crates/searcher`; the link's branch is never asked.
/// A tree holding none of them leaves the row without a directory, said.
#[tokio::test]
async fn an_old_crate_release_is_placed_where_its_own_tree_holds_it() {
    const SHA: &str = "8e8215aa65ac392c31003b62c7989d00e8666304";
    let document = |name: &str, link: &str| {
        json!({
            "crate": {"name": name, "max_stable_version": "1.0.0", "repository": link},
            "versions": [{"num": "1.0.0"}, {"num": "0.4.7", "created_at": "2019-04-15T00:00:00Z"}]
        })
    };
    let info = format!(r#"{{"git":{{"sha1":"{SHA}"}}}}"#);
    let lookup = |name: &'static str, link: &'static str, tree: Vec<&'static str>| {
        let info = info.clone();
        async move {
            let http = RouteHttp::routes(vec![(
                &format!("/api/v1/crates/{name}"),
                document(name, link),
            )])
            .bytes(
                &format!("/crates/{name}/{name}-0.4.7.crate"),
                super::vcs::test_archive(&[(
                    &format!("{name}-0.4.7/.cargo_vcs_info.json"),
                    info.as_bytes(),
                    b'0',
                )]),
            );
            let tags = KnownTags::new(vec![]).with_tree(SHA, tree);
            let b = test_budget();
            let client = RegistryClient {
                tags: Some(&tags),
                ..RegistryClient::new(&http, &b)
            };
            let row = crates(
                &versioned(ArtifactType::Crates, name, "0.4.7"),
                &ArtifactProviderState::default(),
                &client,
            )
            .await
            .expect("row")
            .artifacts
            .remove(0);
            (row, tags.seen())
        }
    };
    let ripgrep = vec![
        "Cargo.toml",
        "ignore/Cargo.toml",
        "grep-searcher/Cargo.toml",
    ];
    let (ignore, seen) = lookup(
        "ignore",
        "https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore",
        ripgrep.clone(),
    )
    .await;
    assert_eq!(ignore.source_ref.as_deref(), Some(SHA));
    assert_eq!(ignore.repository_directory.as_deref(), Some("ignore"));
    assert!(ignore.link_below_ref.is_none());
    assert!(
        !seen.iter().any(|seen| seen.ends_with("@master")),
        "the link's branch is not asked: {seen:?}"
    );
    assert_eq!(
        seen.iter().filter(|seen| seen.contains("^tree@")).count(),
        1,
        "{seen:?}"
    );
    let (searcher, _) = lookup(
        "grep-searcher",
        "https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher",
        ripgrep,
    )
    .await;
    assert_eq!(
        searcher.repository_directory.as_deref(),
        Some("grep-searcher")
    );
    // Where the tree holds the link's own directory, that is it.
    let (current, _) = lookup(
        "ignore",
        "https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore",
        vec!["crates/ignore/Cargo.toml", "ignore/Cargo.toml"],
    )
    .await;
    assert_eq!(
        current.repository_directory.as_deref(),
        Some("crates/ignore")
    );
    let (unplaced, _) = lookup(
        "ignore",
        "https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore",
        vec!["Cargo.toml", "src/lib.rs"],
    )
    .await;
    assert_eq!(unplaced.repository_directory, None);
    assert!(
        unplaced.disclosures.iter().any(|said| said.contains(SHA)
            && said.contains("crates/ignore/, ignore/")
            && said.contains("is not given")),
        "{:?}",
        unplaced.disclosures
    );
}

/// A release whose VCS info names its directory keeps it, whatever the
/// crate's link says; a crate at its repository's root (`path_in_vcs` empty)
/// takes no directory from the link either.
#[tokio::test]
async fn a_recorded_crate_directory_overrides_the_link() {
    for (path_in_vcs, directory) in [("\"grep-searcher\"", Some("grep-searcher")), ("\"\"", None)] {
        let info = format!(
            r#"{{"git":{{"sha1":"0972c6e7c7434aeef16c06aec059c327aca3dc37"}},"path_in_vcs":{path_in_vcs}}}"#
        );
        let http = RouteHttp::routes(vec![(
            "/api/v1/crates/grep-searcher",
            json!({
                "crate": {"name": "grep-searcher", "max_stable_version": "0.1.14",
                    "repository": "https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher"},
                "versions": [{"num": "0.1.6"}]
            }),
        )])
        .bytes(
            "/crates/grep-searcher/grep-searcher-0.1.6.crate",
            super::vcs::test_archive(&[(
                "grep-searcher-0.1.6/.cargo_vcs_info.json",
                info.as_bytes(),
                b'0',
            )]),
        );
        let tags = KnownTags::new(vec![]);
        let b = test_budget();
        let client = RegistryClient {
            tags: Some(&tags),
            ..RegistryClient::new(&http, &b)
        };
        let row = crates(
            &versioned(ArtifactType::Crates, "grep-searcher", "0.1.6"),
            &ArtifactProviderState::default(),
            &client,
        )
        .await
        .expect("row")
        .artifacts
        .remove(0);
        assert_eq!(
            row.repository_directory.as_deref(),
            directory,
            "{path_in_vcs}"
        );
        assert!(row.link_below_ref.is_none(), "{path_in_vcs}");
        assert!(
            !tags.seen().iter().any(|seen| seen.contains("^tree@")),
            "{:?}",
            tags.seen()
        );
    }
}
