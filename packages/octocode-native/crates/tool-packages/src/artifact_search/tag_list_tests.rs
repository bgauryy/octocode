//! npm exact lookups whose release tag only the repository's tag list can
//! settle: what the warning claims when the checks fail, and a tag another
//! package of the repository carries.
use super::*;
use crate::artifact::test_http::KnownTags;
use octocode_test_support::json_fixture;

/// A registry answering one manifest of `pkg` 1.2.1 in `o/r`, with no
/// `gitHead`: the release-tag path pins its source.
async fn registry() -> wiremock::MockServer {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(json_fixture(
                r#"{
                    "name": "pkg",
                    "version": "1.2.1",
                    "repository": {"url": "git+https://github.com/o/r.git"}
                }"#,
            )),
        )
        .mount(&server)
        .await;
    server
}

async fn lookup(server: &wiremock::MockServer, tags: &KnownTags) -> Value {
    let query = typed(&json!({"operation":"resolve", "ecosystem": "npm",
        "packageName": "pkg", "version": "1.2.1",
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())}));
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    execute(
        query,
        ArtifactCall {
            tags: Some(tags),
            ..call(deadline, true)
        },
    )
    .await
    .expect("lookup")
}

/// Every GitHub check fails: no shape was checked, so the warning claims
/// none was, and says why.
#[tokio::test]
async fn a_lookup_whose_every_tag_check_failed_claims_no_checked_shape() {
    let server = registry().await;
    let failing = KnownTags::always(None);
    let data = lookup(&server, &failing).await;
    let warnings = data["warnings"].to_string();
    assert!(!warnings.contains("shapes checked"), "{data}");
    assert!(
        warnings.contains("no tag could be checked on GitHub"),
        "{data}"
    );
}

/// Tags scoped to other packages of the repository name the version (all at
/// one commit, so nothing is ambiguous), and none of this package's does:
/// none is pinned, and the response names them.
#[tokio::test]
async fn another_packages_tag_of_the_version_is_named_never_pinned() {
    let server = registry().await;
    let tags = KnownTags::new(vec![
        "o/r@HEAD",
        "o/r@other@1.2.1=abc",
        "o/r@@scope/other@1.2.1=abc",
        "o/r@packages/other/v1.2.1=abc",
    ])
    .with_tree("abc", vec!["packages/other/package.json"]);
    let data = lookup(&server, &tags).await;
    let row = &data["artifacts"][0];
    assert!(row.get("ref").is_none(), "{data}");
    assert_eq!(row["verification"], "defaultBranch", "{data}");
    let warnings = data["warnings"].to_string();
    for foreign in ["other@1.2.1", "@scope/other@1.2.1", "packages/other/v1.2.1"] {
        assert!(warnings.contains(foreign), "{foreign}: {data}");
    }
    assert!(
        warnings.contains(
            "only tags of other packages (@scope/other@1.2.1, other@1.2.1, packages/other/v1.2.1) name this version"
        ),
        "{data}"
    );
    // A tag of this package's own scoped shape, or one naming no package,
    // is its release.
    for (own, tag) in [
        ("o/r@packages/pkg/v1.2.1", "packages/pkg/v1.2.1"),
        ("o/r@rel/1.2.1", "rel/1.2.1"),
    ] {
        let tags = KnownTags::new(vec!["o/r@HEAD", "o/r@other@1.2.1", own]);
        let data = lookup(&server, &tags).await;
        // `ref` is the commit the tag names; the tag is named apart.
        assert_eq!(
            data["artifacts"][0]["ref"],
            format!("commit-of-{tag}"),
            "{data}"
        );
        assert_eq!(data["artifacts"][0]["verification"], "tag", "{data}");
        assert_eq!(data["artifacts"][0]["releaseTag"], tag, "{data}");
    }
}

/// A word before `-v` or `_v` is no package by its shape alone: a
/// repository-wide release spelling (`release-v1.2.1`) is this package's
/// release tag, pinned, and the response names the tag it took by its name.
#[tokio::test]
async fn a_word_before_v_that_names_no_package_is_the_release_tag() {
    let server = registry().await;
    for tag in ["release-v1.2.1", "release_v1.2.1", "version-v1.2.1"] {
        let listed: &'static str = Box::leak(format!("o/r@{tag}=abc").into_boxed_str());
        let tags = KnownTags::new(vec!["o/r@HEAD", listed]);
        let data = lookup(&server, &tags).await;
        let row = &data["artifacts"][0];
        assert_eq!(row["ref"], "abc", "{data}");
        assert_eq!(row["verification"], "tag", "{data}");
        assert_eq!(row["releaseTag"], tag, "{data}");
        let warnings = data["warnings"].to_string();
        assert!(warnings.contains(tag), "the pin names its tag: {data}");
        assert!(!warnings.contains("other packages"), "{data}");
    }
}

/// `other-v1.2.1` is another package's tag only when the repository holds a
/// package `other` at the tag's commit (a directory of that name with a
/// package manifest); otherwise it is this package's release, said so.
#[tokio::test]
async fn a_stem_is_another_package_only_when_the_repository_holds_it() {
    let server = registry().await;
    let sibling = KnownTags::new(vec!["o/r@HEAD", "o/r@other-v1.2.1=abc"])
        .with_tree("abc", vec!["README.md", "packages/other/package.json"]);
    let data = lookup(&server, &sibling).await;
    assert!(data["artifacts"][0].get("ref").is_none(), "{data}");
    assert!(
        data["warnings"]
            .to_string()
            .contains("only tags of other packages (other-v1.2.1) name this version"),
        "{data}"
    );
    assert!(
        sibling.seen().iter().any(|seen| seen == "o/r^tree@abc"),
        "{:?}",
        sibling.seen()
    );
    // A directory `other` with no package manifest is no package.
    let no_package = KnownTags::new(vec!["o/r@HEAD", "o/r@other-v1.2.1=abc"])
        .with_tree("abc", vec!["docs/other/index.md"]);
    let data = lookup(&server, &no_package).await;
    assert_eq!(data["artifacts"][0]["ref"], "abc", "{data}");
    assert_eq!(data["artifacts"][0]["releaseTag"], "other-v1.2.1", "{data}");
    assert_eq!(data["artifacts"][0]["verification"], "tag", "{data}");
    let warnings = data["warnings"].to_string();
    assert!(warnings.contains("other-v1.2.1"), "{data}");
    assert!(!warnings.contains("other packages"), "{data}");
}

/// `<name>@` is package grammar: another package's tag with no tree read.
#[tokio::test]
async fn a_tag_at_another_name_is_another_packages_with_no_tree_read() {
    let server = registry().await;
    let tags = KnownTags::new(vec!["o/r@HEAD", "o/r@other@1.2.1=abc"]);
    let data = lookup(&server, &tags).await;
    assert!(data["artifacts"][0].get("ref").is_none(), "{data}");
    assert!(
        data["warnings"]
            .to_string()
            .contains("only tags of other packages (other@1.2.1) name this version"),
        "{data}"
    );
    assert!(
        !tags.seen().iter().any(|seen| seen.contains("^tree@")),
        "{:?}",
        tags.seen()
    );
}
