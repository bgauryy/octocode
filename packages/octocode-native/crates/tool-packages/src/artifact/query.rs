//! Registry-facing views over the generated artifactSearch wire query, and
//! the query builders artifact tests share.
#[cfg(test)]
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};

pub mod artifact_type {
    use octocode_contract_types::{ArtifactType, SecurityCheckQueryPackageEcosystem as Ecosystem};

    /// The OSV ecosystem of a registry the artifact provider reads: shared by
    /// artifactSearch's advisory lead and securityCheck's registry gate.
    pub const fn osv_ecosystem(kind: ArtifactType) -> Ecosystem {
        match kind {
            ArtifactType::Npm => Ecosystem::Npm,
            ArtifactType::Pypi => Ecosystem::PyPi,
            ArtifactType::Crates => Ecosystem::CratesIo,
            ArtifactType::Maven => Ecosystem::Maven,
            ArtifactType::Nuget => Ecosystem::NuGet,
            ArtifactType::Go => Ecosystem::Go,
            ArtifactType::Packagist => Ecosystem::Packagist,
            ArtifactType::Rubygems => Ecosystem::RubyGems,
        }
    }

    pub fn as_str(kind: ArtifactType) -> &'static str {
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

/// A requested version as the registry spells it: surrounding whitespace is
/// not part of any version, and a blank one names none.
pub(crate) fn trimmed_version(version: Option<&str>) -> Option<&str> {
    version.map(str::trim).filter(|version| !version.is_empty())
}

/// Registry-facing views over the generated wire query.
pub mod artifact_search_query {
    use octocode_contract_types::{
        ArtifactSearchQuery, ArtifactSearchView, ArtifactType,
        ResolveNugetGoPackagistOrRubygemsEcosystem, ResolvePypiOrCratesEcosystem,
        SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem,
    };

    pub fn artifact_type(query: &ArtifactSearchQuery) -> ArtifactType {
        match query {
            ArtifactSearchQuery::ResolveNpm(_) | ArtifactSearchQuery::SearchNpm(_) => {
                ArtifactType::Npm
            }
            ArtifactSearchQuery::ResolvePypiOrCrates(query) => match query.ecosystem {
                ResolvePypiOrCratesEcosystem::Pypi => ArtifactType::Pypi,
                ResolvePypiOrCratesEcosystem::Crates => ArtifactType::Crates,
            },
            ArtifactSearchQuery::ResolveMaven(_) => ArtifactType::Maven,
            ArtifactSearchQuery::ResolveNugetGoPackagistOrRubygems(query) => {
                match query.ecosystem {
                    ResolveNugetGoPackagistOrRubygemsEcosystem::Nuget => ArtifactType::Nuget,
                    ResolveNugetGoPackagistOrRubygemsEcosystem::Go => ArtifactType::Go,
                    ResolveNugetGoPackagistOrRubygemsEcosystem::Packagist => {
                        ArtifactType::Packagist
                    }
                    ResolveNugetGoPackagistOrRubygemsEcosystem::Rubygems => ArtifactType::Rubygems,
                }
            }
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => match query
                .ecosystem
            {
                SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem::Crates => {
                    ArtifactType::Crates
                }
                SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem::Maven => ArtifactType::Maven,
                SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem::Nuget => ArtifactType::Nuget,
                SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem::Go => ArtifactType::Go,
                SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem::Packagist => {
                    ArtifactType::Packagist
                }
                SearchCratesMavenNugetGoPackagistOrRubygemsEcosystem::Rubygems => {
                    ArtifactType::Rubygems
                }
            },
        }
    }
    /// How much of each row the answer states: the query's `view`, its
    /// contract default when omitted.
    pub fn view(query: &ArtifactSearchQuery) -> ArtifactSearchView {
        match query {
            ArtifactSearchQuery::ResolveNpm(query) => query.view,
            ArtifactSearchQuery::SearchNpm(query) => query.view,
            ArtifactSearchQuery::ResolvePypiOrCrates(query) => query.view,
            ArtifactSearchQuery::ResolveMaven(query) => query.view,
            ArtifactSearchQuery::ResolveNugetGoPackagistOrRubygems(query) => query.view,
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => query.view,
        }
        .unwrap_or_default()
    }
    pub fn package_name(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::ResolveNpm(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::ResolvePypiOrCrates(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::ResolveMaven(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::ResolveNugetGoPackagistOrRubygems(query) => {
                Some(query.package_name.as_str())
            }
            ArtifactSearchQuery::SearchNpm(_)
            | ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(_) => None,
        }
    }
    pub fn version(query: &ArtifactSearchQuery) -> Option<&str> {
        super::trimmed_version(match query {
            ArtifactSearchQuery::ResolveNpm(query) => query.version.as_deref().map(String::as_str),
            ArtifactSearchQuery::ResolvePypiOrCrates(query) => {
                query.version.as_deref().map(String::as_str)
            }
            ArtifactSearchQuery::ResolveMaven(query) => {
                query.version.as_deref().map(String::as_str)
            }
            ArtifactSearchQuery::ResolveNugetGoPackagistOrRubygems(query) => {
                query.version.as_deref().map(String::as_str)
            }
            ArtifactSearchQuery::SearchNpm(_)
            | ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(_) => None,
        })
    }
    pub fn registry_url(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::ResolveNpm(query) => query.registry_url.as_deref(),
            ArtifactSearchQuery::SearchNpm(query) => query.registry_url.as_deref(),
            _ => None,
        }
    }
    pub fn page(query: &ArtifactSearchQuery) -> u64 {
        match query {
            ArtifactSearchQuery::SearchNpm(query) => query.page.map_or(1, std::num::NonZero::get),
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => {
                query.page.map_or(1, std::num::NonZero::get)
            }
            _ => 1,
        }
    }
    pub fn set_page(query: &mut ArtifactSearchQuery, page: u64) {
        let page = std::num::NonZeroU64::new(page);
        match query {
            ArtifactSearchQuery::SearchNpm(query) => {
                (query.page, query.registry_cursor) = (page, None);
            }
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => {
                (query.page, query.registry_cursor) = (page, None);
            }
            _ => {}
        }
    }
    pub(crate) fn cursor(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::SearchNpm(query) => {
                query.registry_cursor.as_deref().map(String::as_str)
            }
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => {
                query.registry_cursor.as_deref().map(String::as_str)
            }
            _ => None,
        }
    }
    pub fn set_cursor(query: &mut ArtifactSearchQuery, cursor: Option<&str>) {
        match query {
            ArtifactSearchQuery::SearchNpm(query) => {
                query.registry_cursor = cursor.and_then(|text| text.parse().ok());
            }
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => {
                query.registry_cursor = cursor.and_then(|text| text.parse().ok());
            }
            _ => {}
        }
    }
    /// Discovery rows per page: the query's `limit`, its contract default
    /// when omitted. A resolve reads one release and pages nothing.
    pub fn page_size(query: &ArtifactSearchQuery) -> usize {
        match query {
            ArtifactSearchQuery::SearchNpm(query) => {
                octocode_engine::num::saturating_usize(query.limit.get())
            }
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => {
                octocode_engine::num::saturating_usize(query.limit.get())
            }
            _ => octocode_runtime_base::contracts::query_limits::artifact_search::search::LIMIT_DEFAULT,
        }
    }
    pub(crate) fn keyword_list(query: &ArtifactSearchQuery) -> Vec<String> {
        match query {
            ArtifactSearchQuery::SearchNpm(query) => {
                query.keywords.iter().map(ToString::to_string).collect()
            }
            ArtifactSearchQuery::SearchCratesMavenNugetGoPackagistOrRubygems(query) => {
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
mod tests;
