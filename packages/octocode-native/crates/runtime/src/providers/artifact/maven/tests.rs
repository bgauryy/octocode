//! Maven exact lookups: POM repositories, dependencies, release days, pinned versions and caching.
use super::super::test_http::{KnownTags, RouteHttp};
use super::super::types::{artifact_query, test_budget};
use super::*;
use serde_json::json;

const METADATA: &str = "/maven2/com/google/guava/guava/maven-metadata.xml";
const GUAVA_METADATA: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "<metadata>",
    "<groupId>com.google.guava</groupId>",
    "<artifactId>guava</artifactId>",
    "<versioning><release>33.7.1-jre</release></versioning>",
    "</metadata>"
);

/// The guava POM path of `version`.
fn pom_path(version: &str) -> String {
    format!("/maven2/com/google/guava/guava/{version}/guava-{version}.pom")
}

fn guava_query() -> ArtifactSearchQuery {
    artifact_query(
        json!({"ecosystem": ArtifactType::Maven, "packageName": "com.google.guava:guava"}),
        None,
    )
}

async fn exact_with(http: &RouteHttp) -> ArtifactItem {
    let b = test_budget();
    let client = RegistryClient::uncached(http, &b);
    let page = maven(&guava_query(), &ArtifactProviderState::default(), &client)
        .await
        .expect("maven exact");
    assert_eq!(page.artifacts.len(), 1);
    page.artifacts.into_iter().next().expect("one artifact")
}

/// The guava metadata and the latest release's POM.
fn guava(pom: &str) -> RouteHttp {
    RouteHttp::new()
        .bytes(METADATA, GUAVA_METADATA)
        .bytes(&pom_path("33.7.1-jre"), pom)
}

#[tokio::test]
async fn maven_exact_maps_scm_url_from_pom() {
    let pom = concat!(
        "<project>",
        "<url>https://guava.dev</url>",
        "<scm><connection>scm:git:git://github.com/google/guava.git</connection>",
        "<url>https://github.com/google/guava.git</url></scm>",
        "</project>"
    );
    let item = exact_with(&guava(pom)).await;
    assert_eq!(item.version.as_deref(), Some("33.7.1-jre"));
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/google/guava")
    );
}

#[tokio::test]
async fn maven_exact_falls_back_to_project_url_without_scm() {
    // Live guava POMs carry no <scm> in the artifact POM (it lives in the
    // parent); the project-level <url> is the upstream link.
    let pom = concat!(
        "<project>",
        "<artifactId>guava</artifactId>",
        "<url>https://github.com/google/guava</url>",
        "</project>"
    );
    let item = exact_with(&guava(pom)).await;
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/google/guava")
    );
}

/// Without the POM nothing it declares is known: no repository and no
/// dependency list (absent, not empty).
#[tokio::test]
async fn maven_exact_survives_missing_pom() {
    let item = exact_with(&RouteHttp::new().bytes(METADATA, GUAVA_METADATA)).await;
    assert_eq!(item.version.as_deref(), Some("33.7.1-jre"));
    assert_eq!(item.repository, None);
    assert_eq!(item.dependencies, None);
    assert_eq!(item.published_at, None);
}

/// An exact row lists the POM's required runtime dependencies as
/// declared (`groupId:artifactId[:version]`): compile and runtime scope,
/// never test, provided or managed ones, nor a dependency's exclusions;
/// optional ones are listed apart. Its release day is the one Central's
/// version search records; a POM without dependencies declares none.
#[tokio::test]
async fn maven_exact_lists_required_and_optional_dependencies_and_the_release_day() {
    let pom = r#"<project>
            <parent><groupId>com.google.guava</groupId><artifactId>guava-parent</artifactId><version>33.7.1-jre</version></parent>
            <dependencyManagement><dependencies>
              <dependency><groupId>managed</groupId><artifactId>only</artifactId><version>1</version></dependency>
            </dependencies></dependencyManagement>
            <dependencies>
              <dependency><groupId>com.google.guava</groupId><artifactId>failureaccess</artifactId><version>1.0.2</version>
                <exclusions><exclusion><groupId>x</groupId><artifactId>excluded</artifactId></exclusion></exclusions>
              </dependency>
              <dependency><groupId>com.google.code.findbugs</groupId><artifactId>jsr305</artifactId></dependency>
              <dependency><groupId>org.runtime</groupId><artifactId>rt</artifactId><version>2</version><scope>runtime</scope></dependency>
              <dependency><groupId>org.opt</groupId><artifactId>opt</artifactId><version>3</version><optional>true</optional></dependency>
              <dependency><groupId>junit</groupId><artifactId>junit</artifactId><version>4</version><scope>test</scope></dependency>
              <dependency><groupId>javax</groupId><artifactId>servlet</artifactId><scope>provided</scope></dependency>
              <!-- <dependency><groupId>commented</groupId><artifactId>out</artifactId></dependency> -->
            </dependencies>
            <build><plugins><plugin><dependencies>
              <dependency><groupId>plugin</groupId><artifactId>dep</artifactId></dependency>
            </dependencies></plugin></plugins></build>
        </project>"#;
    let http = guava(pom).json(
        "/solrsearch/select",
        json!({"response": {"docs": [{"v": "33.7.1-jre", "timestamp": 1_734_386_196_686_i64}]}}),
    );
    let item = exact_with(&http).await;
    assert_eq!(
        item.dependencies,
        Some(vec![
            "com.google.guava:failureaccess:1.0.2".to_owned(),
            "com.google.code.findbugs:jsr305".to_owned(),
            "org.runtime:rt:2".to_owned(),
        ])
    );
    assert_eq!(
        item.optional_dependencies,
        Some(vec!["org.opt:opt:3".to_owned()])
    );
    assert_eq!(item.published_at.as_deref(), Some("2024-12-16"));
    let version = http
        .seen()
        .into_iter()
        .find(|seen| seen.url.path() == "/solrsearch/select")
        .and_then(|seen| {
            seen.url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.into_owned())
        });
    assert_eq!(
        version.as_deref(),
        Some(r#"g:"com.google.guava" AND a:"guava" AND v:"33.7.1-jre""#)
    );

    let bare = exact_with(&guava(
        "<project><url>https://github.com/google/guava</url></project>",
    ))
    .await;
    assert_eq!(bare.dependencies, Some(Vec::new()));
    assert_eq!(bare.optional_dependencies, None);
    // A search row for another version is not this release's day.
    let other = guava("<project/>").json(
        "/solrsearch/select",
        json!({"response": {"docs": [{"v": "1.0", "timestamp": 1_734_386_196_686_i64}]}}),
    );
    assert_eq!(exact_with(&other).await.published_at, None);
}

#[tokio::test]
async fn maven_parses_exact_xml_lookup() {
    let xml = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        "<metadata>",
        "<groupId>com.fasterxml.jackson.core</groupId>",
        "<artifactId>jackson-databind</artifactId>",
        "<versioning><release>2.15.2</release></versioning>",
        "</metadata>"
    );
    let http = RouteHttp::new().bytes(
        "/maven2/com/fasterxml/jackson/core/jackson-databind/maven-metadata.xml",
        xml,
    );
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let q = artifact_query(
        json!({"ecosystem": ArtifactType::Maven, "packageName": "com.fasterxml.jackson.core:jackson-databind"}),
        None,
    );
    let page = maven(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("maven exact");
    assert_eq!(page.artifacts.len(), 1);
    let item = &page.artifacts[0];
    assert_eq!(item.name, "com.fasterxml.jackson.core:jackson-databind");
    assert_eq!(item.version.as_deref(), Some("2.15.2"));
    assert_eq!(
        item.registry_url,
        "https://central.sonatype.com/artifact/com.fasterxml.jackson.core/jackson-databind"
    );
}

/// An exact Maven version reads that version's POM and pins the
/// upstream tag without the `-jre` classifier; an unlisted version is
/// `versionNotFound` with the nearest published versions.
#[tokio::test]
async fn maven_exact_version_reads_that_release_and_its_tag() {
    let metadata = concat!(
        "<metadata><groupId>com.google.guava</groupId><artifactId>guava</artifactId>",
        "<versioning><release>33.7.1-jre</release><versions>",
        "<version>32.1.3-jre</version><version>33.0.0-jre</version><version>33.7.1-jre</version>",
        "</versions></versioning></metadata>"
    );
    let pom = "<project><url>https://github.com/google/guava</url></project>";
    let query = |version: &str| {
        artifact_query(
            json!({"ecosystem": ArtifactType::Maven,
                "packageName": "com.google.guava:guava", "version": version}),
            None,
        )
    };
    let http = RouteHttp::new()
        .bytes(METADATA, metadata)
        .bytes(&pom_path("33.0.0-jre"), pom);
    let b = test_budget();
    let tags = KnownTags::new(vec!["google/guava@v33.0.0"]);
    let client = RegistryClient {
        http: &http,
        budget: &b,
        cache: None,
        tags: Some(&tags),
    };
    let page = maven(
        &query("33.0.0-jre"),
        &ArtifactProviderState::default(),
        &client,
    )
    .await
    .expect("exact version");
    let item = &page.artifacts[0];
    assert_eq!(item.version.as_deref(), Some("33.0.0-jre"));
    assert_eq!(item.source_ref.as_deref(), Some("v33.0.0"));
    assert!(item.source_tag);

    let error = maven(
        &query("33.0.1-jre"),
        &ArtifactProviderState::default(),
        &client,
    )
    .await
    .expect_err("unlisted version");
    assert_eq!(error.code, ErrorCode::VersionNotFound);
    assert!(
        error.nearest.iter().any(|version| version == "33.0.0-jre"),
        "{error:?}"
    );
    assert_eq!(release_version("33.0.0-android"), "33.0.0");
    assert_eq!(release_version("2.0.0-rc1"), "2.0.0-rc1");
}

/// A coordinate that is not `groupId:artifactId` is rejected before any
/// request, naming the field; only a bare artifact name is marked as a
/// name a keyword search finds.
#[tokio::test]
async fn maven_rejects_invalid_coordinate() {
    let http = RouteHttp::new();
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    for (name, searchable) in [("not-a-maven-coordinate", true), ("g:a:1.0", false)] {
        let q = artifact_query(
            json!({"ecosystem": ArtifactType::Maven, "packageName": name}),
            None,
        );
        let err = maven(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect_err("invalid maven coordinate");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(err.message.starts_with("packageName "), "{err:?}");
        assert_eq!(err.searchable_name, searchable, "{name}");
    }
    assert!(http.seen().is_empty());
}

/// Dependency versions written as `${…}` resolve from the POM's own
/// `<properties>` and coordinates (`project.version`, a property naming
/// another); a property it does not define (a parent POM's) stays as
/// written and is named once, never guessed.
#[tokio::test]
async fn maven_dependency_properties_resolve_or_are_named() {
    let pom = r#"<project>
            <parent><groupId>com.google.guava</groupId><artifactId>guava-parent</artifactId><version>33.7.1-jre</version></parent>
            <artifactId>guava</artifactId>
            <properties><base>2.0</base><jsr.version>${base}.1</jsr.version></properties>
            <dependencies>
              <dependency><groupId>com.google.guava</groupId><artifactId>failureaccess</artifactId><version>${failureaccess.version}</version></dependency>
              <dependency><groupId>com.google.guava</groupId><artifactId>sibling</artifactId><version>${project.version}</version></dependency>
              <dependency><groupId>com.google.code.findbugs</groupId><artifactId>jsr305</artifactId><version>${jsr.version}</version></dependency>
              <dependency><groupId>x</groupId><artifactId>again</artifactId><version>${failureaccess.version}</version></dependency>
            </dependencies>
        </project>"#;
    let item = exact_with(&guava(pom)).await;
    assert_eq!(
        item.dependencies,
        Some(vec![
            "com.google.guava:failureaccess:${failureaccess.version}".to_owned(),
            "com.google.guava:sibling:33.7.1-jre".to_owned(),
            "com.google.code.findbugs:jsr305:2.0.1".to_owned(),
            "x:again:${failureaccess.version}".to_owned(),
        ])
    );
    assert_eq!(item.unresolved_properties, ["${failureaccess.version}"]);
}

/// Maven metadata and POMs are XML answers cached like JSON ones: a
/// repeated exact lookup asks the registry nothing.
#[tokio::test]
async fn maven_xml_answers_are_cached() {
    let http = guava("<project><url>https://github.com/google/guava</url></project>");
    let cache = super::super::ArtifactCache::new(None);
    let b = test_budget();
    let client = RegistryClient {
        cache: Some(&cache),
        ..RegistryClient::uncached(&http, &b)
    };
    for _ in 0..2 {
        let page = maven(&guava_query(), &ArtifactProviderState::default(), &client)
            .await
            .expect("maven exact");
        assert_eq!(
            page.artifacts[0].repository.as_deref(),
            Some("https://github.com/google/guava")
        );
    }
    let xml = http
        .seen()
        .iter()
        .filter(|seen| seen.accept == "application/xml")
        .count();
    assert_eq!(xml, 2, "metadata and POM once each");
}
