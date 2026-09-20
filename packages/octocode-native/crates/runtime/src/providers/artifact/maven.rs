use super::http::RegistryClient;
use super::util::{endpoint, object_for, parse_url, required, rows, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactQuery,
    ArtifactType,
};
use regex::Regex;
use url::Url;

pub(crate) async fn maven(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = query.package_name.as_deref() {
        return exact(package_name, client).await;
    }
    let offset = state.offset.unwrap_or(0);
    let size = query.page_size.unwrap_or(10);
    let url = endpoint(
        "https://central.sonatype.com/solrsearch/select",
        &[
            (
                "q",
                Some(
                    query
                        .keywords
                        .as_ref()
                        .map(|v| v.join(" "))
                        .unwrap_or_default(),
                ),
            ),
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

async fn exact(
    package_name: &str,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let parts = package_name.split(':').collect::<Vec<_>>();
    let valid = Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9_.-]*$")
        .map_err(|_| super::util::invalid(ArtifactType::Maven))?;
    if parts.len() != 2 || parts.iter().any(|part| !valid.is_match(part)) {
        return Err(ArtifactError::new(
            "invalid_query",
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
    if Regex::new(r"(?i)<!DOCTYPE|<!ENTITY")
        .map_err(|_| super::util::invalid(ArtifactType::Maven))?
        .is_match(&xml)
    {
        return Err(super::util::invalid(ArtifactType::Maven));
    }
    let clean = Regex::new(r"(?s)<!--.*?-->")
        .map_err(|_| super::util::invalid(ArtifactType::Maven))?
        .replace_all(&xml, "");
    let field = |tag: &str| -> Option<String> {
        Regex::new(&format!(r"<{tag}>\s*([^<]+?)\s*</{tag}>"))
            .ok()?
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
    artifact.version = field("release").or_else(|| field("latest"));
    if let Some(version) = artifact.version.as_deref() {
        artifact.repository = repository_from_pom(&group_path, name, version, client).await?;
    }
    Ok(ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: None,
    })
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
    if Regex::new(r"(?i)<!DOCTYPE|<!ENTITY")
        .map_err(|_| super::util::invalid(ArtifactType::Maven))?
        .is_match(&xml)
    {
        return Ok(None);
    }
    let clean = Regex::new(r"(?s)<!--.*?-->")
        .map_err(|_| super::util::invalid(ArtifactType::Maven))?
        .replace_all(&xml, "");
    let first_url = |text: &str| -> Option<String> {
        Regex::new(r"<url>\s*([^<]+?)\s*</url>")
            .ok()?
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let scm_url = Regex::new(r"(?s)<scm>(.*?)</scm>")
        .map_err(|_| super::util::invalid(ArtifactType::Maven))?
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
    use super::*;
    use crate::providers::RequestBudget;
    use crate::providers::artifact::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
    };
    use std::time::Duration;

    struct MockHttp(Vec<u8>);

    impl ArtifactHttp for MockHttp {
        fn get<'a>(
            &'a self,
            _req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let body = self.0.clone();
            Box::pin(async move { Ok(ArtifactHttpResponse { status: 200, body }) })
        }
    }

    fn budget() -> RequestBudget {
        RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000)
    }

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

    fn guava_query() -> ArtifactQuery {
        ArtifactQuery {
            artifact_type: ArtifactType::Maven,
            package_name: Some("com.google.guava:guava".into()),
            keywords: None,
            page_size: None,
            cursor: None,
            registry: None,
        }
    }

    async fn exact_with(responses: Vec<&str>) -> ArtifactItem {
        let http = SequenceMock::new(responses);
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let page = maven(&guava_query(), &ArtifactProviderState::default(), &client)
            .await
            .expect("maven exact");
        assert_eq!(page.artifacts.len(), 1);
        page.artifacts.into_iter().next().expect("one artifact")
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
        let http = MockHttp(xml.as_bytes().to_vec());
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = ArtifactQuery {
            artifact_type: ArtifactType::Maven,
            package_name: Some("com.fasterxml.jackson.core:jackson-databind".into()),
            keywords: None,
            page_size: None,
            cursor: None,
            registry: None,
        };
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

    #[tokio::test]
    async fn maven_rejects_invalid_coordinate() {
        let http = MockHttp(vec![]);
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
        };
        let q = ArtifactQuery {
            artifact_type: ArtifactType::Maven,
            package_name: Some("not-a-maven-coordinate".into()),
            keywords: None,
            page_size: None,
            cursor: None,
            registry: None,
        };
        let err = maven(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect_err("invalid maven coordinate");
        assert_eq!(err.code, "invalid_query");
    }
}
