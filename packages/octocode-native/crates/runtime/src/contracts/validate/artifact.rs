//! Interpreter for the `artifact_registry_url` opcode: a registry URL is a
//! plain HTTP(S) URL, and a package name is no URL. The query schema's
//! branches enforce `artifact_mode` (one of packageName or keywords, version
//! and page per mode, registryUrl for npm only) before any rule runs.
use super::{ContractValidationError, issue};
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
        let prefix = vec!["queries".into(), index.to_string(), "registryUrl".into()];
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
