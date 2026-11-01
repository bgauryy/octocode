//! Registry-facing views over the generated artifactSearch wire query, and
//! the query builders artifact tests share.
#[cfg(test)]
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};

pub(crate) mod artifact_type {
    use octocode_contract_types::ArtifactType;

    pub(crate) fn as_str(kind: ArtifactType) -> &'static str {
        match kind {
            ArtifactType::Npm => "npm",
            ArtifactType::Pypi => "pypi",
            ArtifactType::Crates => "crates",
            ArtifactType::Maven => "maven",
            ArtifactType::Nuget => "nuget",
            ArtifactType::Go => "go",
            ArtifactType::Packagist => "packagist",
            ArtifactType::Rubygems => "rubygems",
        }
    }
}

/// Registry-facing views over the generated wire query.
pub(crate) mod artifact_search_query {
    use octocode_contract_types::{
        ArtifactSearchQuery, ArtifactSearchQueryResolve2Ecosystem,
        ArtifactSearchQueryResolve4Ecosystem, ArtifactSearchQueryResolve5Ecosystem,
        ArtifactSearchQuerySearch6Ecosystem, ArtifactType,
    };

    pub(crate) fn artifact_type(query: &ArtifactSearchQuery) -> ArtifactType {
        match query {
            ArtifactSearchQuery::Resolve0(_) | ArtifactSearchQuery::Search1(_) => ArtifactType::Npm,
            ArtifactSearchQuery::Resolve2(query) => match query.ecosystem {
                ArtifactSearchQueryResolve2Ecosystem::Pypi => ArtifactType::Pypi,
                ArtifactSearchQueryResolve2Ecosystem::Crates => ArtifactType::Crates,
            },
            ArtifactSearchQuery::Resolve3(_) => ArtifactType::Maven,
            ArtifactSearchQuery::Resolve4(query) => match query.ecosystem {
                ArtifactSearchQueryResolve4Ecosystem::Nuget => ArtifactType::Nuget,
                ArtifactSearchQueryResolve4Ecosystem::Go => ArtifactType::Go,
            },
            ArtifactSearchQuery::Resolve5(query) => match query.ecosystem {
                ArtifactSearchQueryResolve5Ecosystem::Packagist => ArtifactType::Packagist,
                ArtifactSearchQueryResolve5Ecosystem::Rubygems => ArtifactType::Rubygems,
            },
            ArtifactSearchQuery::Search6(query) => match query.ecosystem {
                ArtifactSearchQuerySearch6Ecosystem::Crates => ArtifactType::Crates,
                ArtifactSearchQuerySearch6Ecosystem::Maven => ArtifactType::Maven,
                ArtifactSearchQuerySearch6Ecosystem::Nuget => ArtifactType::Nuget,
                ArtifactSearchQuerySearch6Ecosystem::Go => ArtifactType::Go,
                ArtifactSearchQuerySearch6Ecosystem::Packagist => ArtifactType::Packagist,
                ArtifactSearchQuerySearch6Ecosystem::Rubygems => ArtifactType::Rubygems,
            },
        }
    }
    pub(crate) fn package_name(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::Resolve0(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::Resolve2(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::Resolve3(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::Resolve4(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::Resolve5(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::Search1(_) | ArtifactSearchQuery::Search6(_) => None,
        }
    }
    pub(crate) fn version(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::Resolve0(query) => query.version.as_deref().map(String::as_str),
            ArtifactSearchQuery::Resolve2(query) => query.version.as_deref().map(String::as_str),
            ArtifactSearchQuery::Resolve3(query) => query.version.as_deref().map(String::as_str),
            ArtifactSearchQuery::Resolve4(query) => query.version.as_deref().map(String::as_str),
            ArtifactSearchQuery::Resolve5(_)
            | ArtifactSearchQuery::Search1(_)
            | ArtifactSearchQuery::Search6(_) => None,
        }
    }
    pub fn registry_url(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::Resolve0(query) => query.registry_url.as_deref(),
            ArtifactSearchQuery::Search1(query) => query.registry_url.as_deref(),
            _ => None,
        }
    }
    pub(crate) fn page(query: &ArtifactSearchQuery) -> u64 {
        match query {
            ArtifactSearchQuery::Search1(query) => query.page.map_or(1, std::num::NonZero::get),
            ArtifactSearchQuery::Search6(query) => query.page.map_or(1, std::num::NonZero::get),
            _ => 1,
        }
    }
    pub(crate) fn set_page(query: &mut ArtifactSearchQuery, page: u64) {
        let page = std::num::NonZeroU64::new(page);
        match query {
            ArtifactSearchQuery::Search1(query) => {
                (query.page, query.registry_cursor) = (page, None);
            }
            ArtifactSearchQuery::Search6(query) => {
                (query.page, query.registry_cursor) = (page, None);
            }
            _ => {}
        }
    }
    pub(crate) fn cursor(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::Search1(query) => {
                query.registry_cursor.as_deref().map(String::as_str)
            }
            ArtifactSearchQuery::Search6(query) => {
                query.registry_cursor.as_deref().map(String::as_str)
            }
            _ => None,
        }
    }
    pub(crate) fn set_cursor(query: &mut ArtifactSearchQuery, cursor: Option<&str>) {
        match query {
            ArtifactSearchQuery::Search1(query) => {
                query.registry_cursor = cursor.and_then(|text| text.parse().ok());
            }
            ArtifactSearchQuery::Search6(query) => {
                query.registry_cursor = cursor.and_then(|text| text.parse().ok());
            }
            _ => {}
        }
    }
    /// Discovery rows per page: the query's `limit`, its contract default
    /// when omitted. A resolve reads one release and pages nothing.
    pub(crate) fn page_size(query: &ArtifactSearchQuery) -> usize {
        match query {
            ArtifactSearchQuery::Search1(query) => crate::num::usize_of(query.limit),
            ArtifactSearchQuery::Search6(query) => crate::num::usize_of(query.limit),
            _ => crate::contracts::query_limits::artifact_search::search::LIMIT_DEFAULT,
        }
    }
    pub(crate) fn keyword_list(query: &ArtifactSearchQuery) -> Vec<String> {
        match query {
            ArtifactSearchQuery::Search1(query) => {
                query.keywords.iter().map(ToString::to_string).collect()
            }
            ArtifactSearchQuery::Search6(query) => {
                query.keywords.iter().map(ToString::to_string).collect()
            }
            _ => Vec::new(),
        }
    }
    pub(crate) fn terms(query: &ArtifactSearchQuery) -> String {
        keyword_list(query).join(" ")
    }
}

/// Parses an artifactSearch query from JSON fields layered over `base`
/// (or a minimal npm query); a `null` field removes it.
#[cfg(test)]
pub(crate) fn artifact_query(
    fields: serde_json::Value,
    base: Option<&ArtifactSearchQuery>,
) -> ArtifactSearchQuery {
    let mut value = base.map_or_else(
        || serde_json::json!({"operation":"search","ecosystem": "npm"}),
        |base| serde_json::to_value(base).expect("query serializes"),
    );
    let object = value.as_object_mut().expect("query object");
    for (key, field) in fields.as_object().expect("fields object") {
        if field.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), field.clone());
        }
    }
    serde_json::from_value(value).expect("valid artifactSearch query")
}

/// An exact lookup of `name` at `version`.
#[cfg(test)]
pub(crate) fn versioned(
    artifact_type: ArtifactType,
    name: &str,
    version: &str,
) -> ArtifactSearchQuery {
    artifact_query(
        serde_json::json!({"operation":"resolve","ecosystem": artifact_type, "packageName": name, "version": version}),
        None,
    )
}

/// An exact lookup of `name` (the latest release).
#[cfg(test)]
pub(crate) fn exact_query(artifact_type: ArtifactType, name: &str) -> ArtifactSearchQuery {
    artifact_query(
        serde_json::json!({"operation":"resolve","ecosystem": artifact_type, "packageName": name.to_string()}),
        None,
    )
}

/// A `json` keyword discovery query with `fields` set.
#[cfg(test)]
pub(crate) fn discovery(
    artifact_type: ArtifactType,
    fields: serde_json::Value,
) -> ArtifactSearchQuery {
    let mut query =
        serde_json::json!({"operation":"search","ecosystem": artifact_type, "keywords": ["json"]});
    for (key, value) in fields.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    artifact_query(query, None)
}

#[cfg(test)]
mod tests {
    use super::{artifact_query, artifact_search_query};

    #[test]
    fn canonical_coordinate_and_version_stay_separate() {
        let exact = artifact_query(
            serde_json::json!({
                "operation":"resolve", "packageName":"@scope/package", "version":"^2.0"
            }),
            None,
        );
        assert_eq!(
            artifact_search_query::package_name(&exact),
            Some("@scope/package")
        );
        assert_eq!(artifact_search_query::version(&exact), Some("^2.0"));
        for row in [
            serde_json::json!({"operation":"resolve","ecosystem":"npm","packageName":"package@2.0"}),
            serde_json::json!({"ecosystem":"npm","keywords":["http"]}),
        ] {
            assert!(
                serde_json::from_value::<octocode_contract_types::ArtifactSearchQuery>(row)
                    .is_err()
            );
        }
    }

    /// A cursor names one page's start: moving the page clears it, and a
    /// cursor of any length is carried.
    #[test]
    fn a_cursor_belongs_to_its_page() {
        let mut query = artifact_query(
            serde_json::json!({"operation":"search","ecosystem": "go", "keywords": ["http"], "page": 2, "registryCursor": "c2"}),
            None,
        );
        assert_eq!(artifact_search_query::cursor(&query), Some("c2"));
        artifact_search_query::set_page(&mut query, 3);
        assert_eq!(
            (
                artifact_search_query::page(&query),
                artifact_search_query::cursor(&query)
            ),
            (3, None)
        );
        artifact_search_query::set_cursor(&mut query, Some("c3"));
        assert_eq!(artifact_search_query::cursor(&query), Some("c3"));
        // A registry cursor has no contract ceiling (X14): a long one is carried.
        let long = "x".repeat(20_000);
        artifact_search_query::set_cursor(&mut query, Some(&long));
        assert_eq!(artifact_search_query::cursor(&query), Some(long.as_str()));
        let exact = artifact_query(
            serde_json::json!({"operation":"resolve","packageName": "ajv"}),
            None,
        );
        assert_eq!(artifact_search_query::cursor(&exact), None);
    }
}
