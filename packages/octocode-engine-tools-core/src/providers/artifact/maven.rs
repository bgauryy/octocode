use super::http::RegistryClient;
use super::util::{endpoint, object_for, parse_url, required, rows, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactQuery,
    ArtifactType,
};
use regex::Regex;

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
    Ok(ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: None,
    })
}
