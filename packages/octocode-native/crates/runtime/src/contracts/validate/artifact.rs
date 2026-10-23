//! Interpreter for the `artifact_registry_url` opcode: a registry URL is a
//! plain HTTP(S) URL, and a package name is no URL. The query schema's
//! branches enforce `artifact_mode` (one of packageName or keywords, version
//! and page per mode, registryUrl for npm only) before any rule runs.
use super::{ContractValidationError, issue};
use octocode_contract_types::ArtifactType;
use serde_json::Value;
use url::Url;

pub(super) fn validate_artifact_registry_url(input: &Value) -> Result<(), ContractValidationError> {
    let queries = input["queries"].as_array().ok_or_else(|| {
        issue(
            "schema.queries",
            vec!["queries".into()],
            "Expected queries array",
        )
    })?;
    for (index, query) in queries.iter().enumerate() {
        let prefix = vec!["queries".into(), index.to_string()];
        // A registry page or repository URL is no name any registry looks
        // up: name the fix instead of sending a request that cannot match.
        if let Some(name) = query
            .get("packageName")
            .and_then(Value::as_str)
            .filter(|name| name.contains("://"))
        {
            let shown: String = name.chars().take(80).collect();
            let mut path = prefix.clone();
            path.push("packageName".into());
            return Err(issue(
                "artifact.package-url",
                path,
                format!(
                    "packageName \"{shown}\" is a URL; send the name its manifest uses \
                     (npm @scope/name, Maven group:artifact, a Go module path without the scheme)."
                ),
            ));
        }
        // `name@x` (`name==x` on PyPI) and `version` naming two versions:
        // one would lose silently.
        if let (Some(name), Some(version)) = (
            query.get("packageName").and_then(Value::as_str),
            query.get("version").and_then(Value::as_str).map(str::trim),
        ) && let Some(embedded) = coordinate_version(query)
            && embedded != version
        {
            let mut path = prefix.clone();
            path.push("version".into());
            return Err(issue(
                "artifact.version-conflict",
                path,
                format!(
                    "packageName '{name}' and version '{version}' disagree; set the version once."
                ),
            ));
        }
        if let Some(registry) = query.get("registryUrl").and_then(Value::as_str) {
            let parsed = Url::parse(registry).map_err(|_| {
                issue(
                    "artifact.registry-url",
                    prefix.clone(),
                    "Use an HTTP(S) registry URL without credentials, query, or fragment",
                )
            })?;
            if !matches!(parsed.scheme(), "http" | "https")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
            {
                return Err(issue(
                    "artifact.registry-url",
                    prefix,
                    "Use an HTTP(S) registry URL without credentials, query, or fragment",
                ));
            }
        }
    }
    Ok(())
}

/// The version packageName embeds (`name@x`, PyPI `name==x`), read as the
/// provider reads it when `version` is omitted.
fn coordinate_version(query: &Value) -> Option<&str> {
    let ecosystem = serde_json::from_value(query.get("ecosystem")?.clone()).ok()?;
    split_coordinate(ecosystem, query.get("packageName")?.as_str()?).1
}

/// A package coordinate split into its bare name and embedded version:
/// npm `name@x` and `@scope/name@x`, PyPI `name==x`, crates `name@x`.
pub(crate) fn split_coordinate(ecosystem: ArtifactType, name: &str) -> (&str, Option<&str>) {
    match ecosystem {
        ArtifactType::Npm => split_npm_coordinate(name),
        ArtifactType::Pypi => match name.split_once("==") {
            Some((bare, version)) if !bare.trim().is_empty() && !version.trim().is_empty() => {
                (bare.trim(), Some(version.trim()))
            }
            _ => (name, None),
        },
        ArtifactType::Crates => match name.split_once('@') {
            Some((bare, version)) if !bare.is_empty() && !version.is_empty() => {
                (bare, Some(version))
            }
            _ => (name, None),
        },
        _ => (name, None),
    }
}

fn split_npm_coordinate(package_name: &str) -> (&str, Option<&str>) {
    if let Some(rest) = package_name.strip_prefix('@') {
        if let Some((name, version)) = rest.rsplit_once('@')
            && !version.is_empty()
            && name.contains('/')
        {
            let name_len = package_name.len() - version.len() - 1;
            return (&package_name[..name_len], Some(version));
        }
        return (package_name, None);
    }
    if let Some((name, version)) = package_name.rsplit_once('@')
        && !name.is_empty()
        && !version.is_empty()
        && !name.contains('/')
    {
        return (name, Some(version));
    }
    (package_name, None)
}

#[cfg(test)]
mod tests {
    use crate::contracts::validate::validate;
    use serde_json::json;

    /// A URL sent as the package name is refused before any request, with
    /// the fix named and no repair (only the caller knows the name).
    #[test]
    fn a_url_package_name_is_refused_with_the_fix_named() {
        let input = json!({"queries":[{"ecosystem":"npm",
            "packageName":"https://www.npmjs.com/package/react"}]});
        let error = validate("artifactSearch", input.clone()).expect_err("a URL is no name");
        assert_eq!(error.issues[0].path, ["queries", "0", "packageName"]);
        assert!(
            error.issues[0]
                .message
                .contains("send the name its manifest uses")
        );
        assert!(
            crate::contracts::validate::repair_input(&input, &error.issues, |_| Ok(())).is_none()
        );
        validate(
            "artifactSearch",
            json!({"queries":[{"ecosystem":"go","packageName":"github.com/spf13/cobra"}]}),
        )
        .expect("a Go module path has no scheme");
    }

    /// A version embedded in packageName that disagrees with `version` is
    /// refused: one of them would lose silently.
    #[test]
    fn a_coordinate_version_that_disagrees_with_version_is_refused() {
        let error = validate(
            "artifactSearch",
            json!({"queries":[{"ecosystem":"npm","packageName":"zod@3.22.4","version":"3.22.3"}]}),
        )
        .expect_err("two versions");
        assert_eq!(error.issues[0].path, ["queries", "0", "version"]);
        assert_eq!(
            error.issues[0].message,
            "packageName 'zod@3.22.4' and version '3.22.3' disagree; set the version once."
        );
        for (ecosystem, name) in [
            ("npm", "@scope/name@1.0.0"),
            ("pypi", "requests==2.0.0"),
            ("crates", "serde@1.0.0"),
        ] {
            let input =
                json!({"queries":[{"ecosystem":ecosystem,"packageName":name,"version":"3.0.0"}]});
            assert!(validate("artifactSearch", input).is_err(), "{name}");
        }
        for query in [
            json!({"ecosystem":"npm","packageName":"zod@3.22.4","version":"3.22.4"}),
            json!({"ecosystem":"npm","packageName":"@scope/name","version":"2.0.0"}),
            json!({"ecosystem":"go","packageName":"example.com/m@v1","version":"v2.0.0"}),
        ] {
            validate("artifactSearch", json!({"queries":[query.clone()]}))
                .unwrap_or_else(|error| panic!("{query}: {error:?}"));
        }
    }
}
