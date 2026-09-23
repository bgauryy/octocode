//! Domain dispatch. Interfaces never select security or execution policy.
use super::{ExecutionContext, ExecutionError, FailureKind};
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::{AstGraphQuery, execute_topology};
use crate::tools::ast_rewrite::{AstRewriteRuntimeOptions, execute_ast_rewrite_with_options};
use crate::tools::ast_search::execute_ast;
use crate::tools::local_fetch::{
    LocalFetchRegex, LocalFetchRequest, execute_local_fetch_with_regex,
};
use crate::tools::local_search::{LocalSearchRequest, SearchStatus, execute_local_search};
use serde_json::{Value, json};

pub(super) struct DomainResult {
    pub diagnostics: crate::tools::result::ToolDiagnostics,
    pub data: Value,
    pub cache: bool,
    pub status: Option<&'static str>,
    pub source_digest: Option<String>,
    pub failure: Option<FailureKind>,
}

pub(super) fn execute_local(
    tool: &str,
    query: &Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    regex: &LocalFetchRegex,
    allow_ast_rewrite_apply: bool,
) -> Result<DomainResult, ExecutionError> {
    let mut query = query.clone();
    if let Some(object) = query.as_object_mut() {
        object.remove("goal");
        object.remove("reasoning");
        object.remove("debug");
    }
    match tool {
        "localFetch" => {
            let request: LocalFetchRequest =
                serde_json::from_value(query).map_err(|_| ExecutionError::WorkerFailed)?;
            let result = execute_local_fetch_with_regex(&request, paths, security, context, regex);
            let status = match result.status.as_str() {
                "error" => Some("error"),
                "empty" => Some("empty"),
                _ => None,
            };
            let failure = (status == Some("error")).then_some(if result.resource_missing {
                FailureKind::NotFound
            } else {
                FailureKind::Execution
            });
            let mut data =
                serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
            if status != Some("error")
                && let Some(kind) = crate::content::classify_file_type(&result.path)
            {
                use crate::content::FileType;
                data["fileType"] = json!(match kind {
                    FileType::Code => "code",
                    FileType::Config => "config",
                    FileType::Lock => "lock",
                    FileType::Doc => "doc",
                });
            }
            Ok(DomainResult {
                diagnostics: Default::default(),
                cache: false,
                data,
                status,
                source_digest: result.source_sha256,
                failure,
            })
        }
        "localSearch" => {
            let request: LocalSearchRequest =
                serde_json::from_value(query).map_err(|_| ExecutionError::WorkerFailed)?;
            match execute_local_search(&request, paths, security, context) {
                Ok(mut result) => {
                    for file in &mut result.files {
                        file.path = result
                            .source_root
                            .join(&file.path)
                            .to_string_lossy()
                            .into_owned();
                    }
                    let status = (result.status == SearchStatus::Empty).then_some("empty");
                    let data =
                        serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
                    Ok(DomainResult {
                        diagnostics: Default::default(),
                        cache: false,
                        data,
                        status,
                        source_digest: result.source_snapshot,
                        failure: None,
                    })
                }
                Err(error) => {
                    let mut data =
                        json!({"error":error.message,"errorCode":error.code,"hints":error.hints});
                    if let Some(next) = error.next {
                        data["next"] = *next;
                    }
                    Ok(DomainResult {
                        diagnostics: Default::default(),
                        cache: false,
                        data,
                        status: Some("error"),
                        source_digest: None,
                        failure: Some(FailureKind::Execution),
                    })
                }
            }
        }
        "astSearch" => match execute_ast(query, paths, security, context) {
            Ok(data) => Ok(value_result(data)),
            Err(error) => Ok(domain_error(
                json!({"error":error.message,"errorCode":error.code,"hints":error.hints}),
                error.next,
            )),
        },
        "astTopology" => {
            let request: AstGraphQuery =
                serde_json::from_value(query).map_err(|_| ExecutionError::WorkerFailed)?;
            match execute_topology(&request, paths, security, context) {
                Ok(data) => Ok(value_result(data)),
                Err(error) => {
                    let mut data = json!({"error":error.message,"errorCode":error.code});
                    if !error.hints.is_empty() {
                        data["hints"] = json!(error.hints);
                    }
                    if let Some(next) = error.next {
                        data["next"] = *next;
                    }
                    Ok(domain_error(data, None))
                }
            }
        }
        "astRewrite" => {
            let data = execute_ast_rewrite_with_options(
                query,
                paths,
                security,
                context,
                &AstRewriteRuntimeOptions {
                    allow_apply: allow_ast_rewrite_apply,
                    ..Default::default()
                },
            );
            Ok(value_result(data))
        }
        _ => Err(ExecutionError::WorkerFailed),
    }
}

pub(super) fn value_result(data: Value) -> DomainResult {
    let status = match data.get("status").and_then(Value::as_str) {
        Some("error") => Some("error"),
        Some("empty") => Some("empty"),
        _ => None,
    };
    domain_value(data, status)
}

pub(super) fn provider_failure(
    message: String,
    code: String,
    mut hints: Vec<String>,
    http_status: Option<u16>,
) -> DomainResult {
    let retryable = matches!(code.as_str(), "timeout" | "provider_error")
        || http_status.is_some_and(|status| status == 408 || status == 429 || status >= 500);
    if hints.is_empty() {
        hints.push(
            match code.as_str() {
                "authentication" => "Verify registry credentials and access, then retry.",
                "rate_limit" => "Wait for the provider rate-limit reset before retrying.",
                "timeout" => "Retry once; if it persists, verify registry availability.",
                "invalid_query" => "Correct the package coordinate or query fields.",
                "unsupported_capability" => {
                    "Use the exact package lookup supported by this ecosystem."
                }
                _ if retryable => "Retry once; if it persists, verify registry availability.",
                _ => "Verify provider configuration and the requested package coordinate.",
            }
            .into(),
        );
    }
    let mut data = json!({"error":message,"errorCode":code,"hints":hints,"retryable":retryable});
    // Structured callers need the upstream HTTP status to distinguish e.g. a
    // registry 404 from a 429 without parsing prose (R8; optional — absence
    // is valid).
    if let Some(status) = http_status {
        data["httpStatus"] = json!(status);
    }
    domain_error(data, None)
}

fn domain_value(data: Value, status: Option<&'static str>) -> DomainResult {
    DomainResult {
        diagnostics: Default::default(),
        cache: false,
        data,
        status,
        source_digest: None,
        failure: (status == Some("error")).then_some(FailureKind::Execution),
    }
}

fn domain_error(mut data: Value, next: Option<Box<Value>>) -> DomainResult {
    if let Some(next) = next {
        data["next"] = *next;
    }
    DomainResult {
        diagnostics: Default::default(),
        cache: false,
        data,
        status: Some("error"),
        source_digest: None,
        failure: Some(FailureKind::Execution),
    }
}

#[cfg(test)]
mod provider_failure_tests {
    use super::*;

    #[test]
    fn http_status_is_carried_when_present_and_absent_when_not() {
        let with = provider_failure(
            "upstream said no".into(),
            "provider_error".into(),
            vec![],
            Some(429),
        );
        assert_eq!(with.data["httpStatus"], serde_json::json!(429));
        assert_eq!(with.data["errorCode"], "provider_error");
        assert_eq!(with.data["retryable"], true);
        assert!(with.data["hints"][0].is_string());
        let without = provider_failure("client-side".into(), "invalid_query".into(), vec![], None);
        assert!(without.data.get("httpStatus").is_none(), "absence is valid");
        assert_eq!(without.data["retryable"], false);
        assert!(
            without.data["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("Correct the package coordinate")),
            "{}",
            without.data
        );

        for (code, status, retryable, hint_fragment) in [
            ("authentication", Some(401), false, "credentials"),
            ("rate_limit", Some(429), true, "rate-limit reset"),
            ("timeout", None, true, "Retry once"),
            (
                "unsupported_capability",
                None,
                false,
                "exact package lookup",
            ),
        ] {
            let failure = provider_failure(format!("{code} failure"), code.into(), vec![], status);
            assert_eq!(failure.data["retryable"], retryable, "{code}");
            assert!(
                failure.data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| hint.contains(hint_fragment)),
                "{code}: {}",
                failure.data
            );
        }
    }
}
