//! Bounded source hydration for Jev; retrieval adds evidence but no judgment policy.
use super::ExecutionContext;
use crate::{
    policy::path::PathPolicy,
    security::ContentSecurity,
    tools::{jev_transport::JevProviderError, local_fetch::ContentScan},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Component, Path},
};

const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_TOTAL_BYTES: usize = 256 * 1024;

fn error(message: impl Into<String>) -> JevProviderError {
    JevProviderError { code: "invalidJevSource".into(), message: message.into(), hints: vec!["Provide 1..8 accessible UTF-8 files, at most 64 KiB per file and 256 KiB combined, with optional paired line ranges.".into()] }
}

fn check(context: &ExecutionContext) -> Result<(), JevProviderError> {
    context.check().map_err(|failure| JevProviderError {
        code: match failure {
            super::ExecutionError::Timeout => "timeout",
            _ => "cancelled",
        }
        .into(),
        message: "Source hydration stopped before Jev evaluation.".into(),
        hints: vec![],
    })
}

fn validate(source: &Value) -> Result<(), JevProviderError> {
    let object = source
        .as_object()
        .ok_or_else(|| error("Each source must be an object."))?;
    let kind = source["type"].as_str().unwrap_or_default();
    let allowed: &[&str] = match kind {
        "local" => &["type", "path", "startLine", "endLine"],
        "github" => &[
            "type",
            "owner",
            "repo",
            "path",
            "ref",
            "startLine",
            "endLine",
        ],
        _ => return Err(error("Source type must be local or github.")),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(error("Source contains an unsupported field."));
    }
    let path = source["path"]
        .as_str()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| error("Source path must be nonempty."))?;
    if kind == "local" && !Path::new(path).is_absolute() {
        return Err(error("Local source paths must be absolute."));
    }
    if kind == "github" {
        if path.starts_with('/')
            || path
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
            || path.contains('\\')
            || Path::new(path).components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir
                        | Component::CurDir
                        | Component::RootDir
                        | Component::Prefix(_)
                )
            })
        {
            return Err(error(
                "GitHub source paths must be repository-relative without traversal.",
            ));
        }
        for key in ["owner", "repo", "ref"] {
            let value = source[key]
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| error(format!("GitHub source {key} must be nonempty.")))?;
            if key != "ref"
                && (value.contains('/') || value.contains('\\') || matches!(value, "." | ".."))
            {
                return Err(error("Invalid GitHub owner or repository."));
            }
        }
    }
    match (source.get("startLine"), source.get("endLine")) {
        (None, None) => {}
        (Some(start), Some(end))
            if start
                .as_u64()
                .is_some_and(|start| start > 0 && end.as_u64().is_some_and(|end| end >= start)) => {
        }
        _ => {
            return Err(error(
                "Supply both positive startLine and endLine, with endLine >= startLine.",
            ));
        }
    }
    Ok(())
}

fn select(
    bytes: &[u8],
    source: &Value,
    security: &ContentSecurity,
) -> Result<String, JevProviderError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(error("Source exceeds the 64 KiB raw-file limit."));
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| error("Source must be UTF-8 text."))?;
    if raw.contains('\0') {
        return Err(error("Binary sources are unsupported."));
    }
    let (safe, _) = security
        .sanitize(raw, Path::new(source["path"].as_str().unwrap_or_default()))
        .map_err(|_| error("Source content is blocked by security policy."))?;
    if let (Some(start), Some(end)) = (source["startLine"].as_u64(), source["endLine"].as_u64()) {
        if safe.lines().count() != raw.lines().count() {
            return Err(error(
                "Security redaction changed line mapping; request the whole file.",
            ));
        }
        let lines: Vec<_> = safe.split_inclusive('\n').collect();
        if end > lines.len() as u64 {
            return Err(error(
                "Requested source range exceeds the file; ranges are not truncated.",
            ));
        }
        return Ok(lines[(start - 1) as usize..end as usize].concat());
    }
    Ok(safe)
}

pub(super) fn hydrate(
    query: &Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    local_enabled: bool,
    mut github: impl FnMut(&Value) -> Result<(Vec<u8>, String), JevProviderError>,
) -> Result<(Value, Option<Value>), JevProviderError> {
    let Some(sources) = query.get("sources") else {
        return Ok((query.clone(), None));
    };
    let sources = sources
        .as_object()
        .filter(|sources| (1..=8).contains(&sources.len()))
        .ok_or_else(|| error("Sources must contain 1..8 named references."))?;
    for (id, source) in sources {
        if id.is_empty() {
            return Err(error("Source IDs must be nonempty."));
        }
        validate(source)?;
    }
    let mut hydrated = Map::new();
    let mut receipts = Map::new();
    let mut total = 0usize;
    for (id, source) in sources {
        check(context)?;
        let mut manifest = source.clone();
        let bytes = if source["type"] == "local" {
            if !local_enabled {
                return Err(error("Local source hydration is disabled by local policy."));
            }
            let path = paths
                .validate_read(Path::new(source["path"].as_str().unwrap_or_default()))
                .map_err(|_| {
                    error(format!(
                        "Source {id} is inaccessible or blocked by path policy."
                    ))
                })?;
            let file = std::fs::File::open(&path.canonical)
                .map_err(|_| error(format!("Source {id} cannot be read.")))?;
            if !file
                .metadata()
                .map_err(|_| error("Cannot inspect source metadata."))?
                .is_file()
            {
                return Err(error("Source must be a regular file."));
            }
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| error("Source read failed."))?;
            manifest["path"] = json!(path.canonical);
            bytes
        } else {
            let (bytes, resolved_ref) = github(source)?;
            manifest["resolvedRef"] = json!(resolved_ref);
            bytes
        };
        check(context)?;
        let content = select(&bytes, &manifest, security)?;
        total = total.saturating_add(content.len());
        if total > MAX_TOTAL_BYTES {
            return Err(error("Selected sources exceed the 256 KiB combined limit."));
        }
        receipts.insert(id.clone(), json!({"source":manifest,"contentHash":hex::encode(Sha256::digest(content.as_bytes())),"bytes":content.len()}));
        hydrated.insert(id.clone(), json!({"source":manifest,"content":content}));
    }
    let mut prepared = query.clone();
    prepared
        .as_object_mut()
        .ok_or_else(|| error("Jev query must be an object."))?
        .remove("sources");
    prepared["state"] = json!({"context":query["state"],"sources":hydrated});
    Ok((prepared, Some(Value::Object(receipts))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::path::PathPolicyConfig;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    fn context() -> ExecutionContext {
        ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            output_bytes: 16000,
        }
    }
    fn security() -> ContentSecurity {
        ContentSecurity::new(Arc::new(crate::security::SecurityRegistry::default()))
    }
    fn query(sources: Value) -> Value {
        json!({"state":{"task":"compare"},"questions":{"q":{"type":"noul","instructions":"Assess supplied sources"}},"sources":sources})
    }
    fn policy(root: &Path) -> PathPolicy {
        PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.into()),
            include_home: false,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn multiple_sources_preserve_context_and_return_only_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.rs");
        let b = dir.path().join("b.rs");
        std::fs::write(&a, "first\nsecond\n").unwrap();
        std::fs::write(&b, "other\n").unwrap();
        let request = query(
            json!({"a":{"type":"local","path":a,"startLine":2,"endLine":2},"b":{"type":"local","path":b}}),
        );
        let (result, receipts) = hydrate(
            &request,
            &policy(dir.path()),
            &security(),
            &context(),
            true,
            |_| panic!("unexpected GitHub call"),
        )
        .unwrap();
        assert_eq!(result["state"]["context"], request["state"]);
        assert_eq!(result["state"]["sources"]["a"]["content"], "second\n");
        assert_eq!(result["state"]["sources"]["b"]["content"], "other\n");
        assert!(result.get("sources").is_none());
        let receipts = receipts.unwrap();
        assert_eq!(receipts["a"]["bytes"], 7);
        assert!(!receipts.to_string().contains("second"));
        assert!(receipts["a"].get("content").is_none());
    }

    #[test]
    fn invalid_ranges_and_local_policy_stop_before_github() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        std::fs::write(&path, "one\n").unwrap();
        for source in [
            json!({"type":"local","path":path,"startLine":2,"endLine":1}),
            json!({"type":"local","path":path,"startLine":1}),
            json!({"type":"local","path":path,"startLine":1,"endLine":2}),
        ] {
            assert!(
                hydrate(
                    &query(json!({"a":source})),
                    &policy(dir.path()),
                    &security(),
                    &context(),
                    true,
                    |_| panic!("unexpected GitHub call")
                )
                .is_err()
            );
        }
        let request = query(json!({"a":{"type":"local","path":path}}));
        assert!(
            hydrate(
                &request,
                &policy(dir.path()),
                &security(),
                &context(),
                false,
                |_| panic!("unexpected GitHub call")
            )
            .is_err()
        );
        let outside = tempfile::NamedTempFile::new().unwrap();
        assert!(
            hydrate(
                &query(json!({"a":{"type":"local","path":outside.path()}})),
                &policy(dir.path()),
                &security(),
                &context(),
                true,
                |_| panic!("unexpected GitHub call")
            )
            .is_err()
        );
    }

    #[test]
    fn github_manifest_pins_resolved_ref_and_byte_limits_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let request = query(
            json!({"remote":{"type":"github","owner":"a","repo":"b","path":"src/a.rs","ref":"main"}}),
        );
        let (result, receipt) = hydrate(
            &request,
            &policy(dir.path()),
            &security(),
            &context(),
            true,
            |source| {
                assert_eq!(source["ref"], "main");
                Ok((b"remote\n".to_vec(), "a".repeat(40)))
            },
        )
        .unwrap();
        assert_eq!(
            result["state"]["sources"]["remote"]["source"]["resolvedRef"],
            "a".repeat(40)
        );
        assert_eq!(receipt.unwrap()["remote"]["bytes"], 7);
        assert!(
            hydrate(
                &request,
                &policy(dir.path()),
                &security(),
                &context(),
                true,
                |_| Ok((vec![b'x'; MAX_FILE_BYTES + 1], "a".repeat(40)))
            )
            .is_err()
        );
        assert!(
            hydrate(
                &request,
                &policy(dir.path()),
                &security(),
                &context(),
                true,
                |_| Err(error("missing remote"))
            )
            .is_err()
        );
    }
    #[test]
    fn private_files_are_denied_and_secrets_are_redacted_before_hydration() {
        let dir = tempfile::tempdir().unwrap();
        let secret_file = dir.path().join(".env");
        std::fs::write(&secret_file, "PRIVATE_MARKER").unwrap();
        let denied = query(json!({"private":{"type":"local","path":secret_file}}));
        assert!(
            hydrate(
                &denied,
                &policy(dir.path()),
                &security(),
                &context(),
                true,
                |_| panic!("unexpected GitHub call")
            )
            .is_err()
        );
        let source = dir.path().join("config.rs");
        let token = format!("ghp_{}", "Ab3dEf6hIj9lMn2pQr5tUv8xYz1bCd4fGh7j");
        std::fs::write(&source, format!("const token = \"{token}\";\n")).unwrap();
        let request = query(json!({"safe":{"type":"local","path":source}}));
        let (hydrated, receipts) = hydrate(
            &request,
            &policy(dir.path()),
            &security(),
            &context(),
            true,
            |_| panic!("unexpected GitHub call"),
        )
        .unwrap();
        assert!(!hydrated.to_string().contains(&token));
        assert!(
            hydrated["state"]["sources"]["safe"]["content"]
                .as_str()
                .unwrap()
                .contains("REDACTED")
        );
        assert!(!receipts.unwrap().to_string().contains(&token));
    }
}
