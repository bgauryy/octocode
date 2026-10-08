use super::http::RegistryClient;
use super::util::{endpoint, object_for, parse_url, required, rows, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType,
};
use url::Url;

/// Compile-once XML/coordinate patterns. `maven-metadata.xml` and POM parsing
/// run per lookup; compiling these constant patterns on every call (and once
/// per extracted tag in the old `field` closure) was pure waste.
mod patterns {
    use regex::Regex;
    use std::sync::LazyLock;

    macro_rules! pattern {
        ($name:ident, $src:expr) => {
            pub(super) fn $name() -> &'static Regex {
                static RE: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new($src).expect(concat!("static maven pattern: ", stringify!($name)))
                });
                &RE
            }
        };
    }

    pattern!(valid_coordinate, r"^[A-Za-z0-9_][A-Za-z0-9_.-]*$");
    pattern!(doctype, r"(?i)<!DOCTYPE|<!ENTITY");
    pattern!(comment, r"(?s)<!--.*?-->");
    pattern!(url_tag, r"<url>\s*([^<]+?)\s*</url>");
    pattern!(scm, r"(?s)<scm>(.*?)</scm>");
    pattern!(group_id, r"<groupId>\s*([^<]+?)\s*</groupId>");
    pattern!(artifact_id, r"<artifactId>\s*([^<]+?)\s*</artifactId>");
    pattern!(release, r"<release>\s*([^<]+?)\s*</release>");
    pattern!(latest, r"<latest>\s*([^<]+?)\s*</latest>");
    pattern!(version, r"<version>\s*([^<]+?)\s*</version>");

    /// The bounded set of `<tag>value</tag>` fields extracted from
    /// `maven-metadata.xml`. Unknown tags return `None` (the `regex` crate has
    /// no backreferences, so a single generic `<(\w+)>…</\1>` is unavailable).
    pub(super) fn field(tag: &str) -> Option<&'static Regex> {
        Some(match tag {
            "groupId" => group_id(),
            "artifactId" => artifact_id(),
            "release" => release(),
            "latest" => latest(),
            _ => return None,
        })
    }
}

pub(crate) async fn maven(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = query.package_name() {
        return exact(package_name, query.version(), client).await;
    }
    let offset = state.offset.unwrap_or(0);
    let size = query.page_size().unwrap_or(10);
    let url = endpoint(
        "https://central.sonatype.com/solrsearch/select",
        &[
            ("q", Some(solr_terms(&query.terms()))),
            ("rows", Some(size.to_string())),
            ("start", Some(offset.to_string())),
            ("wt", Some("json".into())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Maven, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Maven))?;
    let data = object_for(
        object_for(&response, ArtifactType::Maven)?
            .get("response")
            .ok_or_else(|| super::util::invalid(ArtifactType::Maven))?,
        ArtifactType::Maven,
    )?;
    let artifacts = rows(
        data.get("docs")
            .ok_or_else(|| super::util::invalid(ArtifactType::Maven))?,
        ArtifactType::Maven,
    )?
    .iter()
    .map(|value| {
        let row = object_for(value, ArtifactType::Maven)?;
        let group = required(row.get("g"), ArtifactType::Maven)?;
        let name = required(row.get("a"), ArtifactType::Maven)?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Maven,
            format!("{group}:{name}"),
            format!(
                "https://central.sonatype.com/artifact/{}/{}",
                super::util::encode_component(&group),
                super::util::encode_component(&name)
            ),
        );
        artifact.version = string(row.get("latestVersion")).or_else(|| string(row.get("v")));
        Ok(artifact)
    })
    .collect::<Result<Vec<_>, ArtifactError>>()?;
    let count = total(data.get("numFound"));
    let more = count
        .map(|count| offset.saturating_add(artifacts.len() as u64) < count)
        .unwrap_or(artifacts.len() == size);
    Ok(ArtifactProviderPage {
        next_state: (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            offset: Some(offset + artifacts.len() as u64),
            ..Default::default()
        }),
        terminal_limit: (more && artifacts.is_empty())
            .then(|| "Maven returned an empty page before its reported total.".into()),
        artifacts,
        total: count,
        registry: None,
    })
}

/// Keyword terms as a Central search query: each word ANDed (the keywords'
/// documented meaning; the endpoint rejects bare whitespace with HTTP 400).
/// Query-syntax characters (field `:`, quotes, groups, wildcards, operators)
/// separate words rather than reach the endpoint, which answers them with
/// 400/404; `-`, `.` and `_` stay, as artifact names use them.
fn solr_terms(terms: &str) -> String {
    terms
        .split(|c: char| c.is_whitespace() || "+&|!(){}[]^\"~*?:\\/".contains(c))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" AND ")
}

async fn exact(
    package_name: &str,
    version: Option<&str>,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let parts = package_name.split(':').collect::<Vec<_>>();
    let valid = patterns::valid_coordinate();
    if parts.len() != 2 || parts.iter().any(|part| !valid.is_match(part)) {
        return Err(ArtifactError::new(
            "invalidInput",
            "Maven packageName must be groupId:artifactId.",
        ));
    }
    let group = parts[0];
    let name = parts[1];
    let group_path = group
        .split('.')
        .map(super::util::encode_component)
        .collect::<Vec<_>>()
        .join("/");
    let url = parse_url(&format!(
        "https://repo.maven.apache.org/maven2/{group_path}/{}/maven-metadata.xml",
        super::util::encode_component(name)
    ))?;
    let Some(xml) = client.text(ArtifactType::Maven, url, true).await? else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    if patterns::doctype().is_match(&xml) {
        return Err(super::util::invalid(ArtifactType::Maven));
    }
    let clean = patterns::comment().replace_all(&xml, "");
    let field = |tag: &str| -> Option<String> {
        patterns::field(tag)?
            .captures(&clean)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let returned_group =
        field("groupId").ok_or_else(|| super::util::invalid(ArtifactType::Maven))?;
    let returned_name =
        field("artifactId").ok_or_else(|| super::util::invalid(ArtifactType::Maven))?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Maven,
        format!("{returned_group}:{returned_name}"),
        format!(
            "https://central.sonatype.com/artifact/{}/{}",
            super::util::encode_component(&returned_group),
            super::util::encode_component(&returned_name)
        ),
    );
    artifact.version = match version.filter(|version| *version != "latest") {
        // An exact version is one the metadata lists (Maven has no ranges
        // here: a bracketed range is not a published version).
        Some(wanted) => {
            let published = patterns::version()
                .captures_iter(&clean)
                .filter_map(|capture| capture.get(1))
                .map(|value| value.as_str().trim().to_owned())
                .collect::<Vec<_>>();
            if !published.iter().any(|version| version == wanted) {
                return Err(super::npm::version_not_found(
                    &artifact.name,
                    wanted,
                    &published,
                ));
            }
            Some(wanted.to_owned())
        }
        None => field("release").or_else(|| field("latest")),
    };
    if let Some(version) = artifact.version.clone() {
        artifact.repository = repository_from_pom(&group_path, name, &version, client).await?;
        // Maven names no commit; an upstream tag of this release pins the
        // lead (a `-jre`/`-android` classifier is not part of the tag).
        if let Some(repository) = artifact.repository.as_deref() {
            artifact.source_ref = super::release_ref::github_release_tag(
                repository,
                release_version(&version),
                client,
            )
            .await;
            artifact.source_tag = artifact.source_ref.is_some();
        }
    }
    Ok(ArtifactProviderPage::single(artifact))
}

/// The release a Maven version names without its variant classifier:
/// `33.0.0-jre` and `33.0.0-android` are both release `33.0.0`. A
/// pre-release qualifier (`2.0.0-rc1`, `1.0-SNAPSHOT`) stays.
fn release_version(version: &str) -> &str {
    version
        .rsplit_once('-')
        .filter(|(_, classifier)| matches!(*classifier, "jre" | "android"))
        .map_or(version, |(release, _)| release)
}

/// Upstream source link for exact lookups, taken from the versioned POM's
/// `<scm><url>` with the project-level `<url>` as fallback. Keyword rows stay
/// link-less: the solr search response carries no scm metadata. Enrichment is
/// best-effort — a missing POM or suspicious XML skips the link instead of
/// failing a lookup that already resolved.
async fn repository_from_pom(
    group_path: &str,
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<String>, ArtifactError> {
    let encoded_name = super::util::encode_component(name);
    let encoded_version = super::util::encode_component(version);
    let url = parse_url(&format!(
        "https://repo1.maven.org/maven2/{group_path}/{encoded_name}/{encoded_version}/{encoded_name}-{encoded_version}.pom"
    ))?;
    let Some(xml) = client.text(ArtifactType::Maven, url, true).await? else {
        return Ok(None);
    };
    if patterns::doctype().is_match(&xml) {
        return Ok(None);
    }
    let clean = patterns::comment().replace_all(&xml, "");
    let first_url = |text: &str| -> Option<String> {
        patterns::url_tag()
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let scm_url = patterns::scm()
        .captures(&clean)
        .and_then(|capture| capture.get(1))
        .and_then(|block| first_url(block.as_str()));
    Ok(scm_url
        .or_else(|| first_url(&clean))
        .and_then(|value| super::npm::normalize_repository(&value))
        // Drop scm:/git-protocol leftovers the normalizer cannot canonicalize.
        .filter(|value| {
            Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
        }))
}

#[cfg(test)]
mod tests {
    use super::super::types::{StaticHttp, artifact_query, test_budget};
    use super::*;
    use crate::providers::RequestBudget;
    use crate::providers::artifact::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
    };

    /// Returns pre-set responses in insertion order; further calls 404.
    struct SequenceMock {
        responses: Vec<Vec<u8>>,
        index: std::sync::atomic::AtomicUsize,
    }

    impl SequenceMock {
        fn new(responses: Vec<&str>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|v| v.as_bytes().to_vec())
                    .collect(),
                index: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl ArtifactHttp for SequenceMock {
        fn get<'a>(
            &'a self,
            _req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body = self.responses.get(idx).cloned().unwrap_or_default();
            let status = if idx < self.responses.len() { 200 } else { 404 };
            Box::pin(async move { Ok(ArtifactHttpResponse { status, body }) })
        }
    }

    const GUAVA_METADATA: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        "<metadata>",
        "<groupId>com.google.guava</groupId>",
        "<artifactId>guava</artifactId>",
        "<versioning><release>33.7.1-jre</release></versioning>",
        "</metadata>"
    );

    fn guava_query() -> ArtifactSearchQuery {
        artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Maven, "packageName": "com.google.guava:guava".to_string()}),
            None,
        )
    }

    async fn exact_with(responses: Vec<&str>) -> ArtifactItem {
        let http = SequenceMock::new(responses);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let page = maven(&guava_query(), &ArtifactProviderState::default(), &client)
            .await
            .expect("maven exact");
        assert_eq!(page.artifacts.len(), 1);
        page.artifacts.into_iter().next().expect("one artifact")
    }

    /// Records each request URL and answers with an empty search page.
    struct CaptureMock(std::sync::Mutex<Vec<Url>>);

    impl ArtifactHttp for CaptureMock {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            if let Ok(mut urls) = self.0.lock() {
                urls.push(req.url.clone());
            }
            Box::pin(async move {
                Ok(ArtifactHttpResponse {
                    status: 200,
                    body: br#"{"response":{"numFound":0,"docs":[]}}"#.to_vec(),
                })
            })
        }
    }

    /// QA2: Maven Central's Solr search answers HTTP 400 to whitespace-
    /// separated terms ("markdown parser"), which surfaced as an
    /// `invalidInput` row for a schema-valid query. Terms are ANDed (the
    /// documented keyword semantics); query-syntax characters split words.
    #[tokio::test]
    async fn maven_discovery_ands_terms_and_splits_query_syntax() {
        for (keywords, expected) in [
            (
                serde_json::json!(["markdown parser"]),
                "markdown AND parser",
            ),
            (
                serde_json::json!(["markdown", "parser"]),
                "markdown AND parser",
            ),
            (serde_json::json!(["a:b (c)"]), "a AND b AND c"),
            (serde_json::json!(["spring-boot"]), "spring-boot"),
            (serde_json::json!(["guava"]), "guava"),
        ] {
            let http = CaptureMock(std::sync::Mutex::new(Vec::new()));
            let b = test_budget();
            let client = RegistryClient::uncached(&http, &b);
            let query = artifact_query(
                serde_json::json!({"ecosystem": ArtifactType::Maven, "keywords": keywords}),
                None,
            );
            maven(&query, &ArtifactProviderState::default(), &client)
                .await
                .expect("maven search");
            let urls = http.0.lock().expect("urls");
            let q = urls[0]
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.into_owned());
            assert_eq!(q.as_deref(), Some(expected), "{keywords}");
        }
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
        let item = exact_with(vec![GUAVA_METADATA, pom]).await;
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
        let item = exact_with(vec![GUAVA_METADATA, pom]).await;
        assert_eq!(
            item.repository.as_deref(),
            Some("https://github.com/google/guava")
        );
    }

    #[tokio::test]
    async fn maven_exact_survives_missing_pom() {
        // Only the metadata response is provided; the POM fetch 404s.
        let item = exact_with(vec![GUAVA_METADATA]).await;
        assert_eq!(item.version.as_deref(), Some("33.7.1-jre"));
        assert_eq!(item.repository, None);
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
        let http = StaticHttp(xml.as_bytes().to_vec());
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Maven, "packageName": "com.fasterxml.jackson.core:jackson-databind".to_string()}),
            None,
        );
        let page = maven(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("maven exact");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "com.fasterxml.jackson.core:jackson-databind");
        assert_eq!(item.version.as_deref(), Some("2.15.2"));
        assert!(
            item.registry_url.contains("sonatype.com"),
            "{}",
            item.registry_url
        );
    }

    /// Answers a tag check for one tag only.
    struct OneTag(&'static str);
    impl super::super::ReleaseTags for OneTag {
        fn exists<'a>(
            &'a self,
            _owner: &'a str,
            _repo: &'a str,
            tag: &'a str,
        ) -> super::super::TagFuture<'a> {
            let found = tag == self.0;
            Box::pin(async move { Some(found) })
        }
    }

    /// AR4: an exact Maven version reads that version's POM and pins the
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
        let query = artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Maven,
                "packageName": "com.google.guava:guava".to_string(), "version": "33.0.0-jre"}),
            None,
        );
        let http = SequenceMock::new(vec![metadata, pom]);
        let b = test_budget();
        let tags = OneTag("v33.0.0");
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: None,
            tags: Some(&tags),
        };
        let page = maven(&query, &ArtifactProviderState::default(), &client)
            .await
            .expect("exact version");
        let item = &page.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("33.0.0-jre"));
        assert_eq!(item.source_ref.as_deref(), Some("v33.0.0"));
        assert!(item.source_tag);

        let missing = artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Maven,
                "packageName": "com.google.guava:guava".to_string(), "version": "33.0.1-jre"}),
            None,
        );
        let http = SequenceMock::new(vec![metadata]);
        let client = RegistryClient::uncached(&http, &b);
        let error = maven(&missing, &ArtifactProviderState::default(), &client)
            .await
            .expect_err("unlisted version");
        assert_eq!(error.code, "versionNotFound");
        assert_eq!(release_version("33.0.0-android"), "33.0.0");
        assert_eq!(release_version("2.0.0-rc1"), "2.0.0-rc1");
    }

    #[tokio::test]
    async fn maven_rejects_invalid_coordinate() {
        let http = StaticHttp(vec![]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Maven, "packageName": "not-a-maven-coordinate".to_string()}),
            None,
        );
        let err = maven(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect_err("invalid maven coordinate");
        assert_eq!(err.code, "invalidInput");
    }
}
