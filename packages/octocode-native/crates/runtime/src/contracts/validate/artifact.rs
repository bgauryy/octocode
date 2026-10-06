//! Interpreter for the `artifact_mode` and `artifact_registry_url` opcodes.
use super::{ContractValidationError, issue};
use serde_json::Value;
use url::Url;

pub(super) fn validate_artifact_queries(input: &Value) -> Result<(), ContractValidationError> {
    let queries = input["queries"].as_array().ok_or_else(|| {
        issue(
            "schema.queries",
            vec!["queries".into()],
            "Expected queries array",
        )
    })?;
    for (index, query) in queries.iter().enumerate() {
        let prefix = vec!["queries".into(), index.to_string()];
        let exact = query.get("packageName").is_some();
        let discovery = query.get("keywords").is_some();
        if exact == discovery {
            return Err(issue(
                "artifact.mode",
                prefix,
                "Set exactly one of packageName or keywords",
            ));
        }
        if !exact && query.get("version").is_some() {
            return Err(issue(
                "artifact.version",
                prefix,
                "version applies only to exact packageName lookups",
            ));
        }
        if exact
            && ["pageSize", "page"]
                .iter()
                .any(|field| query.get(field).is_some())
        {
            return Err(issue(
                "artifact.exact-pagination",
                prefix,
                "pageSize and page apply only to keyword discovery",
            ));
        }
        if let Some(registry) = query.get("registry").and_then(Value::as_str) {
            if query.get("type").and_then(Value::as_str) != Some("npm") {
                return Err(issue(
                    "artifact.registry-type",
                    prefix,
                    "registry is supported only for type:npm",
                ));
            }
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
