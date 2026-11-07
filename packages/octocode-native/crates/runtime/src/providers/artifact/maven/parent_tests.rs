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
    resolve_on(http, tags).await
}

async fn resolve_on(
    http: &dyn super::super::http::ArtifactHttp,
    tags: Option<&KnownTags>,
) -> ArtifactItem {
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
/// Apache gitbox address (mirrored to `apache/logging-log4j2`). Its declared
/// tag is the release candidate's (`log4j-2.14.1-rc1`), which names no
/// release 2.14.1: it is not pinned and claims no tag verification. Its
/// prefix is the stem asked first (`log4j-2.14.1`), then the usual tags.
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
    let tags = KnownTags::new(vec![
        "apache/logging-log4j2@log4j-2.14.1-rc1",
        "apache/logging-log4j2@rel/2.14.1",
    ]);
    let item = resolve(&http, Some(&tags)).await;
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/apache/logging-log4j2")
    );
    // No tag names 2.14.1 (`rel/2.14.1` follows no rule a declared tag or
    // the usual shapes give): the declared tag, which exists, is pinned as
    // the registry's claim, never as a checked release tag.
    assert_eq!(item.source_ref.as_deref(), Some("log4j-2.14.1-rc1"));
    assert!(!item.source_tag, "{:?}", item.source_ref);
    assert!(item.declared_ref, "the pin is disclosed as declared");
    let seen = tags.seen();
    assert_eq!(seen[0], "apache/logging-log4j2@log4j-2.14.1", "{seen:?}");
    assert_eq!(
        seen.last().map(String::as_str),
        Some("apache/logging-log4j2@log4j-2.14.1-rc1"),
        "the declared tag is asked only after every release candidate: {seen:?}"
    );
}

/// A POM with its own `<scm>` still takes the scope its parent manages:
/// the walk reads ancestors until every unscoped dependency finds its
/// managed entry, not only until the source link is found.
#[tokio::test]
async fn a_child_with_its_own_scm_takes_the_scope_its_parent_manages() {
    let core = format!(
        "<project>{LOG4J_PARENT}<artifactId>log4j-core</artifactId>\
        <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
        <dependencies>\
        <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId></dependency>\
        <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId></dependency>\
        </dependencies></project>"
    );
    let parent = "<project><version>2.14.1</version><dependencyManagement><dependencies>\
        <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId>\
        <version>5.7.0</version><scope>test</scope></dependency>\
        </dependencies></dependencyManagement></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core.as_str())
        .bytes(PARENT_POM, parent);
    let item = resolve(&http, None).await;
    assert_eq!(
        item.dependencies,
        Some(vec!["org.slf4j:slf4j-api".to_owned()])
    );
}

/// A `<scope>import</scope>` BOM in `<dependencyManagement>` manages the
/// scope as Maven applies it: after the parent chain, read once.
#[tokio::test]
async fn an_imported_bom_manages_the_scope() {
    const BOM: &str = "/maven2/org/junit/junit-bom/5.7.0/junit-bom-5.7.0.pom";
    let core = "<project><artifactId>log4j-core</artifactId><version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
        <properties><junit.version>5.7.0</junit.version></properties>\
        <dependencyManagement><dependencies>\
        <dependency><groupId>org.junit</groupId><artifactId>junit-bom</artifactId>\
        <version>${junit.version}</version><type>pom</type><scope>import</scope></dependency>\
        </dependencies></dependencyManagement>\
        <dependencies>\
        <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId></dependency>\
        <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId></dependency>\
        </dependencies></project>";
    let bom = "<project><dependencyManagement><dependencies>\
        <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId>\
        <version>5.7.0</version><scope>test</scope></dependency>\
        </dependencies></dependencyManagement></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core)
        .bytes(BOM, bom);
    let item = resolve(&http, None).await;
    assert_eq!(
        item.dependencies,
        Some(vec!["org.slf4j:slf4j-api".to_owned()])
    );
    let boms = http.targets().into_iter().filter(|t| t == BOM).count();
    assert_eq!(boms, 1, "the BOM is read once");
}

/// The registry path of BOM `x:{artifact}:{version}`.
fn bom_path(artifact: &str, version: &str) -> String {
    format!("/maven2/x/{artifact}/{version}/{artifact}-{version}.pom")
}

fn import_of(artifact: &str, version: &str) -> String {
    format!(
        "<dependency><groupId>x</groupId><artifactId>{artifact}</artifactId>\
         <version>{version}</version><type>pom</type><scope>import</scope></dependency>"
    )
}

fn parent_of(artifact: &str, version: &str) -> String {
    format!(
        "<parent><groupId>x</groupId><artifactId>{artifact}</artifactId>\
         <version>{version}</version></parent>"
    )
}

/// A management entry for `d:e` at `scope`.
fn manages_at(scope: &str) -> String {
    format!(
        "<dependency><groupId>d</groupId><artifactId>e</artifactId><version>1</version>\
         <scope>{scope}</scope></dependency>"
    )
}

/// A BOM POM: its `<parent>`, `<properties>` and management `entries`.
fn bom_pom(parent: &str, properties: &str, entries: &str) -> String {
    format!(
        "<project>{parent}<properties>{properties}</properties>\
         <dependencyManagement><dependencies>{entries}</dependencies></dependencyManagement></project>"
    )
}

/// log4j-core with its own `<scm>`, importing `imports`, depending on `d:e`
/// (scope and optional left to management) and on slf4j (both stated).
fn importing_core(imports: &str) -> String {
    format!(
        "<project><artifactId>log4j-core</artifactId><version>2.14.1</version>\
         <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
         <dependencyManagement><dependencies>{imports}</dependencies></dependencyManagement>\
         <dependencies><dependency><groupId>d</groupId><artifactId>e</artifactId></dependency>\
         <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId>\
         <scope>compile</scope><optional>false</optional></dependency></dependencies></project>"
    )
}

fn slf4j_only() -> Option<Vec<String>> {
    Some(vec!["org.slf4j:slf4j-api".to_owned()])
}

/// A BOM resolves its coordinates with its own parents' properties: the
/// import `q-bom:${x.version}` its parent defines is read, and the scope it
/// manages applies.
#[tokio::test]
async fn a_bom_resolves_its_import_with_its_parents_properties() {
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, importing_core(&import_of("a-bom", "1")))
        .bytes(
            &bom_path("a-bom", "1"),
            bom_pom(
                &parent_of("a-parent", "1"),
                "",
                &import_of("q-bom", "${x.version}"),
            ),
        )
        .bytes(
            &bom_path("a-parent", "1"),
            bom_pom("", "<x.version>2</x.version>", ""),
        )
        .bytes(
            &bom_path("q-bom", "2"),
            bom_pom("", "", &manages_at("test")),
        );
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, slf4j_only());
    assert!(item.disclosures.is_empty(), "{:?}", item.disclosures);
    let q = bom_path("q-bom", "2");
    assert_eq!(http.targets().iter().filter(|t| **t == q).count(), 1);
}

/// An import whose coordinate keeps a property no POM read defines cannot be
/// read: it is stated, never dropped silently.
#[tokio::test]
async fn an_import_naming_an_undefined_property_is_disclosed() {
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, importing_core(&import_of("a-bom", "1")))
        .bytes(
            &bom_path("a-bom", "1"),
            bom_pom("", "", &import_of("q-bom", "${nowhere}")),
        );
    let item = resolve(&http, None).await;
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    let said = &item.disclosures[0];
    assert!(
        said.contains("x:q-bom:${nowhere}") && said.contains("scope"),
        "{said}"
    );
}

/// Maven applies imports depth-first: a BOM that X imports counts as
/// defined in X, before the next import Y
/// (https://maven.apache.org/guides/introduction/introduction-to-dependency-mechanism.html#importing-dependencies).
/// Q, imported by X, manages `d:e` as test; Y manages it as runtime: Q wins,
/// even though Y is read first and already manages it.
#[tokio::test]
async fn a_bom_an_earlier_import_brings_manages_before_a_later_import() {
    let imports = format!("{}{}", import_of("x-bom", "1"), import_of("y-bom", "1"));
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, importing_core(&imports))
        .bytes(
            &bom_path("x-bom", "1"),
            bom_pom("", "", &import_of("q-bom", "1")),
        )
        .bytes(
            &bom_path("y-bom", "1"),
            bom_pom(
                "",
                "",
                &format!("{}{}", manages_at("runtime"), import_of("q2-bom", "1")),
            ),
        )
        .bytes(
            &bom_path("q-bom", "1"),
            bom_pom("", "", &manages_at("test")),
        )
        .bytes(
            &bom_path("q2-bom", "1"),
            bom_pom("", "", &manages_at("runtime")),
        );
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, slf4j_only());
}

/// Maven interpolates the effective model: a property the child overrides
/// wins in the management its parent declares, both in a managed
/// coordinate (`d:${a}`) and in an import (`q-bom:${v}`).
#[tokio::test]
async fn a_child_property_overrides_its_parents_in_inherited_management() {
    let core = format!(
        "<project>{LOG4J_PARENT}<artifactId>log4j-core</artifactId><version>2.14.1</version>\
         <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
         <properties><v>2</v><a>e</a></properties>\
         <dependencies><dependency><groupId>d</groupId><artifactId>e</artifactId></dependency>\
         <dependency><groupId>d</groupId><artifactId>f</artifactId></dependency>\
         <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId>\
         <scope>compile</scope><optional>false</optional></dependency></dependencies></project>"
    );
    let parent = bom_pom(
        "",
        "<v>1</v><a>other</a>",
        &format!(
            "<dependency><groupId>d</groupId><artifactId>${{a}}</artifactId><scope>test</scope></dependency>{}",
            import_of("q-bom", "${v}")
        ),
    );
    let manages_f = |scope: &str| {
        format!(
            "<dependency><groupId>d</groupId><artifactId>f</artifactId><scope>{scope}</scope></dependency>"
        )
    };
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core)
        .bytes(PARENT_POM, parent)
        .bytes(
            &bom_path("q-bom", "1"),
            bom_pom("", "", &manages_f("runtime")),
        )
        .bytes(&bom_path("q-bom", "2"), bom_pom("", "", &manages_f("test")));
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, slf4j_only());
    assert!(!http.targets().contains(&bom_path("q-bom", "1")));
}

/// The same inside an imported BOM: the BOM's own value of a property its
/// parent's management names wins.
#[tokio::test]
async fn a_bom_property_overrides_its_parents_in_the_management_it_inherits() {
    let manages_f = "<dependency><groupId>d</groupId><artifactId>f</artifactId><scope>test</scope></dependency>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, importing_core(&import_of("a-bom", "1")))
        .bytes(
            &bom_path("a-bom", "1"),
            bom_pom(&parent_of("a-parent", "1"), "<v>2</v>", ""),
        )
        .bytes(
            &bom_path("a-parent", "1"),
            bom_pom("", "<v>1</v>", &import_of("q-bom", "${v}")),
        )
        .bytes(
            &bom_path("q-bom", "2"),
            bom_pom("", "", &manages_at("test")),
        )
        .bytes(&bom_path("q-bom", "1"), bom_pom("", "", manages_f));
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, slf4j_only());
    assert!(!http.targets().contains(&bom_path("q-bom", "1")));
}

/// An imported BOM's parent the Maven repository lacks is stated: a scope it
/// would manage is unknown, not absent.
#[tokio::test]
async fn a_bom_parent_missing_from_the_repository_is_disclosed() {
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, importing_core(&import_of("b-bom", "1")))
        .bytes(
            &bom_path("b-bom", "1"),
            bom_pom(&parent_of("b-parent", "1"), "", ""),
        );
    let item = resolve(&http, None).await;
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    let said = &item.disclosures[0];
    assert!(
        said.contains("x:b-parent:1")
            && said.contains("x:b-bom:1")
            && said.contains("not in the Maven repository")
            && said.contains("scope"),
        "{said}"
    );
}

/// Answers like `inner`, but holds each request for a `gated` target until
/// every gated target has been asked: a walk that reads them one at a time
/// never finishes.
struct Gate {
    inner: RouteHttp,
    gated: Vec<String>,
    arrived: std::sync::atomic::AtomicUsize,
}

impl super::super::http::ArtifactHttp for Gate {
    fn get<'a>(
        &'a self,
        request: super::super::http::ArtifactHttpRequest,
        budget: &'a octocode_io::budget::RequestBudget,
    ) -> super::super::http::ArtifactHttpFuture<'a> {
        use std::sync::atomic::Ordering;
        let gated = self.gated.iter().any(|target| target == request.url.path());
        let answer = self.inner.get(request, budget);
        Box::pin(async move {
            if gated {
                self.arrived.fetch_add(1, Ordering::SeqCst);
                while self.arrived.load(Ordering::SeqCst) < self.gated.len() {
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            }
            answer.await
        })
    }
}

/// Sibling imports are fetched together, then applied in declared order.
#[tokio::test]
async fn sibling_imports_are_fetched_together() {
    let imports = format!("{}{}", import_of("s1-bom", "1"), import_of("s2-bom", "1"));
    let http = Gate {
        inner: RouteHttp::new()
            .bytes(METADATA, METADATA_XML)
            .bytes(CORE_POM, importing_core(&imports))
            .bytes(
                &bom_path("s1-bom", "1"),
                bom_pom("", "", &manages_at("test")),
            )
            .bytes(
                &bom_path("s2-bom", "1"),
                bom_pom("", "", &manages_at("runtime")),
            ),
        gated: vec![bom_path("s1-bom", "1"), bom_path("s2-bom", "1")],
        arrived: std::sync::atomic::AtomicUsize::new(0),
    };
    // The bound only turns a serial walk's hang into a failure.
    let item = tokio::time::timeout(std::time::Duration::from_secs(10), resolve_on(&http, None))
        .await
        .expect("the sibling imports are asked together");
    assert_eq!(item.dependencies, slf4j_only(), "s1 manages first");
}

/// Each unread BOM line says what happened: a POM refused for its DOCTYPE is
/// not one the repository lacks, and an unread BOM parent leaves a managed
/// scope unknown, not a source link.
#[tokio::test]
async fn an_unread_bom_says_what_happened() {
    let imports = format!("{}{}", import_of("bad-bom", "1"), import_of("b-bom", "1"));
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, importing_core(&imports))
        .bytes(
            &bom_path("bad-bom", "1"),
            "<!DOCTYPE project [<!ENTITY e \"x\">]><project/>",
        )
        .bytes(
            &bom_path("b-bom", "1"),
            bom_pom(&parent_of("b-parent", "1"), "", ""),
        )
        .status(&bom_path("b-parent", "1"), 503, json!({}));
    let item = resolve(&http, None).await;
    assert_eq!(item.disclosures.len(), 2, "{:?}", item.disclosures);
    let (bad, parent) = (&item.disclosures[0], &item.disclosures[1]);
    assert!(
        bad.contains("x:bad-bom:1") && bad.contains("DOCTYPE") && !bad.contains("not in the"),
        "{bad}"
    );
    assert!(
        parent.contains("x:b-parent:1")
            && parent.contains("x:b-bom:1")
            && parent.contains("scope")
            && !parent.contains("source link"),
        "{parent}"
    );
}

/// A dependency whose stated scope is no runtime one needs no managed
/// `<optional>`: the walk reads no parent for it.
#[tokio::test]
async fn a_test_scoped_dependency_reads_no_parent() {
    let core = format!(
        "<project>{LOG4J_PARENT}<artifactId>log4j-core</artifactId><version>2.14.1</version>\
         <scm><url>https://github.com/apache/logging-log4j2</url></scm>\
         <dependencies><dependency><groupId>d</groupId><artifactId>e</artifactId>\
         <scope>test</scope></dependency></dependencies></project>"
    );
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, core)
        .bytes(PARENT_POM, "<project/>");
    let item = resolve(&http, None).await;
    assert_eq!(item.dependencies, Some(Vec::new()));
    assert!(
        !http.targets().iter().any(|t| t == PARENT_POM),
        "{:?}",
        http.targets()
    );
}

/// log4j-core 2.26.1's chain declares `<tag>rel/2.25.3</tag>` (a stale tag
/// upstream left in the 2.26.1 bom). That tag exists but names another
/// release: its prefix gives `rel/2.26.1`, which pins the source.
#[tokio::test]
async fn a_declared_tag_of_another_release_lends_only_its_prefix() {
    let metadata = concat!(
        "<metadata><groupId>org.apache.logging.log4j</groupId>",
        "<artifactId>log4j-core</artifactId>",
        "<versioning><release>2.26.1</release></versioning></metadata>"
    );
    let parent = "<parent><groupId>org.apache.logging.log4j</groupId>\
        <artifactId>log4j</artifactId><version>2.26.1</version></parent>";
    let declaring = "<project><version>2.26.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url>\
        <tag>rel/2.25.3</tag></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, metadata)
        .bytes(
            "/maven2/org/apache/logging/log4j/log4j-core/2.26.1/log4j-core-2.26.1.pom",
            module(parent),
        )
        .bytes(
            "/maven2/org/apache/logging/log4j/log4j/2.26.1/log4j-2.26.1.pom",
            declaring,
        );
    let tags = KnownTags::new(vec![
        "apache/logging-log4j2@rel/2.25.3",
        "apache/logging-log4j2@rel/2.26.1",
    ]);
    let item = resolve(&http, Some(&tags)).await;
    assert_eq!(item.source_ref.as_deref(), Some("rel/2.26.1"));
    assert!(item.source_tag);
    assert_eq!(tags.seen()[0], "apache/logging-log4j2@rel/2.26.1");
    assert!(
        !tags.seen().iter().any(|tag| tag.ends_with("@rel/2.25.3")),
        "{:?}",
        tags.seen()
    );
}

/// A declared tag whose trailing version is this release pins it as is.
#[tokio::test]
async fn a_declared_tag_naming_the_release_is_asked_first() {
    let parent = "<project><version>2.14.1</version>\
        <scm><url>https://github.com/apache/logging-log4j2</url>\
        <tag>rel/2.14.1</tag></scm></project>";
    let http = RouteHttp::new()
        .bytes(METADATA, METADATA_XML)
        .bytes(CORE_POM, module(LOG4J_PARENT))
        .bytes(PARENT_POM, parent);
    let tags = KnownTags::new(vec!["apache/logging-log4j2@rel/2.14.1"]);
    let item = resolve(&http, Some(&tags)).await;
    assert_eq!(item.source_ref.as_deref(), Some("rel/2.14.1"));
    assert!(item.source_tag);
    assert_eq!(tags.seen()[0], "apache/logging-log4j2@rel/2.14.1");
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
        <version>${a.version}</version><scope>compile</scope><optional>false</optional>\
        </dependency></dependencies></project>";
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
