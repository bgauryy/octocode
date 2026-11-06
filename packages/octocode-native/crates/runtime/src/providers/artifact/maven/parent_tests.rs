//! A module POM names its source through its `<parent>` chain.
use super::super::query::artifact_query;
use super::super::test_http::test_budget;
use super::super::test_http::{KnownTags, RouteHttp};
use super::*;
use octocode_contract_types::ErrorCode;
use serde_json::json;

const METADATA: &str = "/maven2/org/apache/logging/log4j/log4j-core/maven-metadata.xml";
const METADATA_XML: &str = concat!(
    "<metadata><groupId>org.apache.logging.log4j</groupId>",
    "<artifactId>log4j-core</artifactId>",
    "<versioning><release>2.14.1</release></versioning></metadata>"
);
const CORE_POM: &str = "/maven2/org/apache/logging/log4j/log4j-core/2.14.1/log4j-core-2.14.1.pom";
const PARENT_POM: &str = "/maven2/org/apache/logging/log4j/log4j/2.14.1/log4j-2.14.1.pom";
const ROOT_POM: &str = "/maven2/org/apache/logging/logging-parent/3/logging-parent-3.pom";

/// A module POM with no `<scm>` of its own, under `parent`.
fn module(parent: &str) -> String {
    format!(
        "<project><modelVersion>4.0.0</modelVersion>{parent}\
         <artifactId>log4j-core</artifactId>\
         <issueManagement><url>https://issues.apache.org/jira/browse/LOG4J2</url></issueManagement>\
         </project>"
    )
}

const LOG4J_PARENT: &str = "<parent><groupId>org.apache.logging.log4j</groupId>\
    <artifactId>log4j</artifactId><version>2.14.1</version></parent>";

fn log4j_query() -> ArtifactSearchQuery {
    artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Maven,
            "packageName": "org.apache.logging.log4j:log4j-core"}),
        None,
    )
}

async fn resolve(http: &RouteHttp, tags: Option<&KnownTags>) -> ArtifactItem {
    let budget = test_budget();
    let client = RegistryClient {
        tags: tags.map(|tags| tags as &dyn super::super::ReleaseTags),
        ..RegistryClient::new(http, &budget)
    };
    let page = maven(&log4j_query(), &ArtifactProviderState::default(), &client)
        .await
        .expect("maven exact");
    page.artifacts.into_iter().next().expect("one artifact")
}

/// log4j-core 2.14.1 declares no `<scm>`: its parent `log4j` does, at an
/// Apache gitbox address (mirrored to `apache/logging-log4j2`) with the tag
/// the release was cut from.
#[tokio::test]
async fn a_module_pom_without_scm_takes_the_source_of_its_parent() {
    let parent = "<project><parent><groupId>org.apache.logging</groupId>\
        <artifactId>logging-parent</artifactId><version>3</version></parent>\
        <version>2.14.1</version>\
        <url>https://logging.apache.org/log4j/2.x/</url>\
        <scm><connection>scm:git:https://gitbox.apache.org/repos/asf/logging-log4j2.git</connection>\
        <url>https://gitbox.apache.org/repos/asf?p=logging-log4j2.git</url>\
        <tag>log4j-2.14.1-rc1</tag></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .bytes(PARENT_POM, parent);
    let tags = KnownTags::new(vec!["apache/logging-log4j2@log4j-2.14.1-rc1"]);
    let item = resolve(&http, Some(&tags)).await;
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/apache/logging-log4j2")
    );
    // The tag the parent declares for this very version is probed first and
    // pins the source; it is not guessed.
    assert_eq!(item.source_ref.as_deref(), Some("log4j-2.14.1-rc1"));
    assert!(item.source_tag);
    assert_eq!(tags.seen()[0], "apache/logging-log4j2@log4j-2.14.1-rc1");
}

/// A tag declared by an ancestor of another version says nothing about this
/// one, and `HEAD` is no tag: the probe falls back to the plain tags.
#[tokio::test]
async fn a_declared_tag_of_another_version_or_head_is_not_a_candidate() {
    let parent = "<project><version>2.14.0</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url><tag>log4j-2.14.0</tag></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .bytes(PARENT_POM, parent);
    let tags = KnownTags::new(vec!["apache/logging-log4j2@v2.14.1"]);
    let item = resolve(&http, Some(&tags)).await;
    assert_eq!(item.source_ref.as_deref(), Some("v2.14.1"));
    assert!(
        !tags
            .seen()
            .iter()
            .any(|seen| seen.ends_with("@log4j-2.14.0")),
        "{:?}",
        tags.seen()
    );
    let head = "<project><version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url><tag>HEAD</tag></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .bytes(PARENT_POM, head);
    let tags = KnownTags::new(vec![]);
    resolve(&http, Some(&tags)).await;
    assert_eq!(tags.seen()[0], "apache/logging-log4j2@v2.14.1");
}

/// The walk goes up while no POM names a source, reads each POM once, and
/// ends at a root without one: the row has no repository and says so.
#[tokio::test]
async fn a_chain_with_no_scm_ends_without_a_repository() {
    let middle = "<project><parent><groupId>org.apache.logging</groupId>\
        <artifactId>logging-parent</artifactId><version>3</version></parent>\
        <version>2.14.1</version></project>";
    let root = "<project><version>3</version></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .bytes(PARENT_POM, middle)
        .bytes(ROOT_POM, root);
    let item = resolve(&http, None).await;
    assert_eq!(item.repository, None);
    let poms = http
        .targets()
        .into_iter()
        .filter(|target| target.ends_with(".pom"))
        .collect::<Vec<_>>();
    assert_eq!(poms, [CORE_POM, PARENT_POM, ROOT_POM]);
}

/// A parent cycle (or a parent that is the POM itself) ends the walk.
#[tokio::test]
async fn a_parent_cycle_ends_the_walk() {
    let cyclic = "<project><parent><groupId>org.apache.logging.log4j</groupId>\
        <artifactId>log4j-core</artifactId><version>2.14.1</version></parent></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, cyclic);
    let item = resolve(&http, None).await;
    assert_eq!(item.repository, None);
}

/// An `<scm><url>` into a module directory names the repository and the
/// directory, the way the GitHub tools take them.
#[tokio::test]
async fn an_scm_url_into_a_module_directory_names_the_repository_and_the_directory() {
    let scm = "<project><version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2/tree/2.x/log4j-core</url></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, scm);
    let tags = KnownTags::new(vec!["apache/logging-log4j2@2.x"]);
    let page = super::super::test_http::execute(&http, &log4j_query(), Some(&tags))
        .await
        .expect("maven exact");
    let item = &page.artifacts[0];
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/apache/logging-log4j2")
    );
    assert_eq!(item.repository_directory.as_deref(), Some("log4j-core"));
}

/// A dependency version written as a property resolves from the nearest POM
/// up the `<parent>` chain that declares it, past the POM that names the
/// source: log4j-core's `${woodstox.version}` is the `log4j` parent's, a
/// nearer declaration overrides a farther one, and a property no POM of the
/// chain declares stays named.
#[tokio::test]
async fn dependency_properties_resolve_over_the_parent_chain() {
    let core = "<project><modelVersion>4.0.0</modelVersion>\
        <parent><groupId>org.apache.logging.log4j</groupId>\
        <artifactId>log4j</artifactId><version>2.14.1</version></parent>\
        <artifactId>log4j-core</artifactId>\
        <dependencies>\
        <dependency><groupId>com.fasterxml.woodstox</groupId><artifactId>woodstox-core</artifactId>\
        <version>${woodstox.version}</version><optional>true</optional></dependency>\
        <dependency><groupId>x</groupId><artifactId>far</artifactId><version>${far.version}</version></dependency>\
        <dependency><groupId>x</groupId><artifactId>none</artifactId><version>${none.version}</version></dependency>\
        <dependency><groupId>x</groupId><artifactId>shadow</artifactId><version>${shadow.version}</version></dependency>\
        </dependencies></project>";
    let middle = "<project><parent><groupId>org.apache.logging</groupId>\
        <artifactId>logging-parent</artifactId><version>3</version></parent>\
        <version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
        <developers><developer><id>a</id>\
        <properties><picUrl>http://example.org/a.jpg</picUrl></properties>\
        </developer></developers>\
        <properties><woodstox.version>5.3.0</woodstox.version>\
        <shadow.version>near</shadow.version></properties></project>";
    let root = "<project><version>3</version>\
        <properties><far.version>9.1</far.version><shadow.version>far</shadow.version></properties></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core)
        .bytes(PARENT_POM, middle)
        .bytes(ROOT_POM, root);
    let item = resolve(&http, None).await;
    assert_eq!(
        item.optional_dependencies,
        Some(vec![
            "com.fasterxml.woodstox:woodstox-core:5.3.0".to_owned()
        ])
    );
    assert_eq!(
        item.dependencies,
        Some(vec![
            "x:far:9.1".to_owned(),
            "x:none:${none.version}".to_owned(),
            "x:shadow:near".to_owned()
        ])
    );
    assert_eq!(item.unresolved_properties, ["${none.version}"]);
    // The chain is read once, to its end only because `far`/`none` needed it.
    let poms = http
        .targets()
        .into_iter()
        .filter(|target| target.ends_with(".pom"))
        .collect::<Vec<_>>();
    assert_eq!(poms, [CORE_POM, PARENT_POM, ROOT_POM]);
}

/// A chain that is not needed is not read: the walk stops at the POM that
/// resolves the last property.
#[tokio::test]
async fn the_parent_chain_is_read_no_further_than_the_properties_need() {
    let core = "<project><parent><groupId>org.apache.logging.log4j</groupId>\
        <artifactId>log4j</artifactId><version>2.14.1</version></parent>\
        <dependencies><dependency><groupId>x</groupId><artifactId>a</artifactId>\
        <version>${a.version}</version></dependency></dependencies></project>";
    let middle = "<project><parent><groupId>org.apache.logging</groupId>\
        <artifactId>logging-parent</artifactId><version>3</version></parent>\
        <version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
        <properties><a.version>1</a.version></properties></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core)
        .bytes(PARENT_POM, middle);
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, Some(vec!["x:a:1".to_owned()]));
    assert!(item.unresolved_properties.is_empty());
    assert!(
        !http.targets().iter().any(|target| target == ROOT_POM),
        "{:?}",
        http.targets()
    );
}

/// A parent POM that exists but cannot be read ends the chain with a statement
/// naming it; the lookup that already resolved does not fail, and a missing
/// source link is not read as "no scm".
#[tokio::test]
async fn a_parent_pom_that_cannot_be_read_ends_the_chain_with_a_disclosure() {
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .status(PARENT_POM, 503, json!({"error": "unavailable"}));
    let item = resolve(&http, None).await;
    assert_eq!(item.repository, None);
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    assert!(
        item.disclosures[0].contains("org.apache.logging.log4j:log4j:2.14.1"),
        "{:?}",
        item.disclosures
    );
}

/// A cancelled call fails the lookup: a parent POM request that is cancelled is
/// not an unread parent to disclose.
#[tokio::test]
async fn a_cancelled_parent_pom_request_fails_the_lookup() {
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .fails(PARENT_POM, ErrorCode::Cancelled);
    let error = super::super::test_http::execute(&http, &log4j_query(), None)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, ErrorCode::Cancelled);
}

/// The same for the walk that only resolves properties: the dependency stays
/// as declared and the unread parent is named.
#[tokio::test]
async fn a_parent_pom_unread_while_resolving_properties_is_disclosed() {
    let core = "<project><parent><groupId>org.apache.logging.log4j</groupId>\
        <artifactId>log4j</artifactId><version>2.14.1</version></parent>\
        <dependencies><dependency><groupId>x</groupId><artifactId>a</artifactId>\
        <version>${a.version}</version></dependency></dependencies></project>";
    let middle = "<project><parent><groupId>org.apache.logging</groupId>\
        <artifactId>logging-parent</artifactId><version>3</version></parent>\
        <version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core)
        .bytes(PARENT_POM, middle)
        .status(ROOT_POM, 500, json!({}));
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, Some(vec!["x:a:${a.version}".to_owned()]));
    assert_eq!(item.unresolved_properties, ["${a.version}"]);
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    assert!(
        item.disclosures[0].contains("org.apache.logging:logging-parent:3"),
        "{:?}",
        item.disclosures
    );
}

/// A parent the repository does not have is the end of a chain, not a failure
/// to disclose.
#[tokio::test]
async fn a_parent_pom_the_repository_lacks_is_no_disclosure() {
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT));
    let item = resolve(&http, None).await;
    assert!(item.disclosures.is_empty(), "{:?}", item.disclosures);
}

/// Only the project's own `<dependencies>`, `<properties>` and coordinates
/// count: a developer's `<properties>`, a profile's `<dependencies>` and a
/// plugin's or managed `<version>` before them shadow nothing.
#[tokio::test]
async fn the_projects_own_blocks_are_not_shadowed_by_nested_ones() {
    let core = "<project><modelVersion>4.0.0</modelVersion>\
        <developers><developer><id>a</id>\
        <properties><dep.version>wrong</dep.version></properties></developer></developers>\
        <profiles><profile><id>p</id><dependencies>\
        <dependency><groupId>x</groupId><artifactId>profiled</artifactId><version>1</version></dependency>\
        </dependencies></profile></profiles>\
        <dependencyManagement><dependencies><dependency><groupId>x</groupId>\
        <artifactId>managed</artifactId><version>2</version></dependency></dependencies></dependencyManagement>\
        <build><plugins><plugin><groupId>p</groupId><artifactId>plug</artifactId><version>0.0.1</version>\
        <dependencies><dependency><groupId>x</groupId><artifactId>plugged</artifactId><version>3</version></dependency></dependencies>\
        </plugin></plugins></build>\
        <groupId>org.apache.logging.log4j</groupId><artifactId>log4j-core</artifactId>\
        <version>2.14.1</version>\
        <properties><dep.version>right</dep.version></properties>\
        <dependencies><dependency><groupId>x</groupId><artifactId>own</artifactId>\
        <version>${dep.version}-${project.version}</version></dependency></dependencies></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core);
    let item = resolve(&http, None).await;
    assert_eq!(
        item.dependencies,
        Some(vec!["x:own:right-2.14.1".to_owned()])
    );
}
