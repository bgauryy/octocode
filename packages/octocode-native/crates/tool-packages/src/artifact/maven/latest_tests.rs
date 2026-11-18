//! What an unversioned Maven lookup calls latest: the newest stable release.
use super::super::query::artifact_query;
use super::super::test_http::RouteHttp;
use super::super::test_http::test_budget;
use super::*;
use serde_json::json;

const METADATA: &str = "/maven2/com/google/guava/guava/maven-metadata.xml";

/// The guava POM path of `version`.
fn pom_path(version: &str) -> String {
    format!("/maven2/com/google/guava/guava/{version}/guava-{version}.pom")
}

async fn exact_with(http: &RouteHttp) -> ArtifactItem {
    let b = test_budget();
    let client = RegistryClient::new(http, &b);
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Maven, "packageName": "com.google.guava:guava"}),
        None,
    );
    let page = maven(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("maven exact");
    page.artifacts.into_iter().next().expect("one artifact")
}

/// `<release>` is the last deployment, which may be a milestone or a release
/// candidate: Maven's own `RELEASE` keyword would pick it, but "latest" is the
/// newest stable release the metadata lists. A stable `<release>` stands, even
/// past a date-shaped version that orders higher numerically.
#[tokio::test]
async fn maven_latest_is_the_newest_stable_release() {
    let metadata = |release: &str, versions: &[&str]| {
        let listed = versions
            .iter()
            .map(|version| format!("<version>{version}</version>"))
            .collect::<String>();
        format!(
            "<metadata><groupId>com.google.guava</groupId><artifactId>guava</artifactId>\
             <versioning><release>{release}</release><versions>{listed}</versions></versioning></metadata>"
        )
    };
    for (release, versions, expected) in [
        (
            "7.1.0-RC1",
            &["7.0.9", "7.0.10", "7.1.0-M1", "7.1.0-M2", "7.1.0-RC1"][..],
            "7.0.10",
        ),
        (
            "5.0.0.Alpha2",
            &["4.2.20.Final", "5.0.0.Alpha1", "5.0.0.Alpha2"][..],
            "4.2.20.Final",
        ),
        (
            "3.0.0-beta3",
            &["2.26.1", "3.0.0-alpha1", "3.0.0-beta3"][..],
            "2.26.1",
        ),
        ("2.0-SNAPSHOT", &["1.9", "2.0-SNAPSHOT"][..], "1.9"),
        // A stable release stands.
        (
            "33.8.0-jre",
            &["33.7.2-jre", "33.8.0-android", "33.8.0-jre"][..],
            "33.8.0-jre",
        ),
        (
            "2.22.0",
            &["20030203.000550", "2.21.0", "2.22.0"][..],
            "2.22.0",
        ),
        // No stable release at all: the newest there is.
        ("1.0.0-rc1", &["1.0.0-beta1", "1.0.0-rc1"][..], "1.0.0-rc1"),
    ] {
        let http = RouteHttp::new()
            .bytes(METADATA, metadata(release, versions))
            .bytes(&pom_path(expected), "<project/>");
        let item = exact_with(&http).await;
        assert_eq!(item.version.as_deref(), Some(expected), "{release}");
    }
}

/// What Maven's `ComparableVersion` treats as a prerelease qualifier, plus the
/// common pre-final words it does not know; the rest are final releases.
#[tokio::test]
async fn maven_reads_prerelease_qualifiers_case_blind_and_keeps_final_ones() {
    for (version, prerelease) in [
        ("1.0-alpha-1", true),
        ("1.0.Alpha2", true),
        ("1.0-a1", true),
        ("1.0-b2", true),
        ("1.0-Beta", true),
        ("1.0-M3", true),
        ("1.0-milestone-1", true),
        ("1.0-RC1", true),
        ("1.0.CR2", true),
        ("1.0-rc.1", true),
        ("1.0-SNAPSHOT", true),
        ("1.0-preview", true),
        ("1.0-pre3", true),
        ("1.0-EA2", true),
        ("1.0-dev", true),
        ("1.0", false),
        ("1.0.Final", false),
        ("1.0.RELEASE", false),
        ("1.0-GA", false),
        ("1.0.SP1", false),
        ("33.0-jre", false),
        ("33.0-android", false),
        ("1.0-incubating", false),
        ("r09", false),
        ("1.0-b", false),
        ("2.0-mysql8", false),
        ("1.2.3.4-alpine", false),
    ] {
        let metadata = format!(
            "<metadata><groupId>com.google.guava</groupId><artifactId>guava</artifactId>\
             <versioning><release>{version}</release><versions>\
             <version>0.5</version><version>{version}</version></versions></versioning></metadata>"
        );
        let expected = if prerelease { "0.5" } else { version };
        let http = RouteHttp::new()
            .bytes(METADATA, metadata)
            .bytes(&pom_path(expected), "<project/>");
        let item = exact_with(&http).await;
        assert_eq!(item.version.as_deref(), Some(expected), "{version}");
    }
}
