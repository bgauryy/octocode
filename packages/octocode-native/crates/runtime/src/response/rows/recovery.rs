use crate::response::output::{SANDBOX_HINT, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

pub(crate) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::response::result::ToolDiagnostics,
) {
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    for reason in diagnostics.partial_reasons {
        crate::response::paging::mark_partial(data, reason, &[]);
    }
    append(data, "hints", diagnostics.hints);
}

fn append(data: &mut serde_json::Map<String, Value>, key: &str, values: Vec<String>) {
    if values.is_empty() {
        return;
    }
    let target = data.entry(key).or_insert_with(|| json!([]));
    if let Some(existing) = target.as_array_mut() {
        for value in values {
            let value = Value::String(value);
            if !existing.contains(&value) {
                existing.push(value);
            }
        }
    }
}

/// Authentication recovery shared by every GitHub tool.
pub(crate) const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// The one recovery for a cancelled call, on every tool and stage.
pub(crate) const CANCELLED_HINT: &str = "Retry only if the result is still needed and what cancelled it changed; otherwise report the cancellation.";

const TRANSIENT_HINT: &str = "Retry once only if retryable is true; if it persists, check provider availability and report the coverage gap.";

/// Fill missing guidance without replacing producer-specific recovery or
/// repeating it. Call before the output security gate so query-derived facts
/// and future tool-owned guidance share the normal redaction boundary.
pub(crate) fn attach_error_hint(tool: &dyn ToolOutput, data: &mut Value, query: &Value) {
    if let Some(hint) = missing_error_hint(tool, data, query) {
        data["hints"] = json!([hint]);
    }
}

/// [`attach_error_hint`] for a row of a batch: the shared recovery rides
/// `notices`, so the stage states it once per response, on the first error
/// row that needs it, instead of on every failed row
/// ([`crate::response::notices::place`]).
pub(crate) fn attach_error_notice(tool: &dyn ToolOutput, data: &mut Value, query: &Value) {
    if let Some(hint) = missing_error_hint(tool, data, query) {
        crate::response::paging::push_notice(data, hint);
    }
}

/// The shared recovery an error row without its own hints needs.
fn missing_error_hint(tool: &dyn ToolOutput, data: &Value, query: &Value) -> Option<&'static str> {
    if data
        .get("hints")
        .and_then(Value::as_array)
        .is_some_and(|hints| !hints.is_empty())
    {
        return None;
    }
    let code = crate::id::error_code_of(data)?;
    error_code_hint(tool, code, query).filter(|hint| !hint.is_empty())
}

/// The one recovery table for the `errorCode` values every tool shares
/// (provider kinds, policy, locks, input). A tool answers only for the codes
/// it alone emits, or its own timeout ([`ToolOutput::error_hint`]); any other
/// code gets the tool's fallback hint.
pub(crate) fn error_code_hint(
    tool: &dyn ToolOutput,
    code: ErrorCode,
    query: &Value,
) -> Option<&'static str> {
    Some(match code {
        ErrorCode::OutsideAllowedRoots | ErrorCode::SymlinkEscape => SANDBOX_HINT,
        ErrorCode::Authentication => GITHUB_AUTH_RECOVERY_HINT,
        ErrorCode::Permission | ErrorCode::PermissionDenied => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        ErrorCode::RateLimited | ErrorCode::ClassificationRateLimited => {
            "Wait for Retry-After or the rate-limit reset before retrying."
        }
        // A tool that knows why its own calls time out says so.
        ErrorCode::Timeout => tool
            .error_hint(code, query)
            .unwrap_or("Retry once with a smaller scope or fewer independent queries; if it times out again, report unavailable coverage."),
        ErrorCode::Transport | ErrorCode::Server => TRANSIENT_HINT,
        ErrorCode::Cancelled => CANCELLED_HINT,
        ErrorCode::ResponseTooLarge | ErrorCode::ScopeTooBroad => {
            "Narrow paths, ranges or filters; split independent scopes into separate queries. State what remains unchecked."
        }
        ErrorCode::RedirectDenied => "Use the canonical allowed host and repository.",
        ErrorCode::Decode => {
            "Retry once; report a provider response incompatibility if it persists."
        }
        ErrorCode::Configuration | ErrorCode::Config => {
            "Inspect octocode config --json and correct the setting named in the error before retrying."
        }
        ErrorCode::CredentialStoreUnavailable => {
            "Restore access to the credential store or supply the provider token through its supported environment setting."
        }
        ErrorCode::Unavailable => {
            "The provider blocks this resource for legal reasons; retrying will not help."
        }
        ErrorCode::HttpStatus => {
            "Check httpStatus and the message; this status is not a network failure."
        }
        // One code, one recovery on every tool.
        ErrorCode::PathNotFound => crate::response::output::VERIFY_PATH_HINT,
        ErrorCode::PathPolicyDenied => crate::policy::discovery::WITHHELD_HINT,
        ErrorCode::InvalidGlob => "Correct the glob named in the error; globs select paths, while regexEngine selects text-pattern syntax. Do not change the intended scope.",
        code => {
            if let Some(hint) = tool.error_hint(code, query) {
                hint
            } else if let Some(hint) = shared_execution_hint(code) {
                hint
            } else if crate::id::error_codes::class(code)
                == crate::id::error_codes::ErrorClass::InvalidInput
            {
                "Input rejected: fix only the field the error names, as its message shows; broadening will not help."
            } else {
                return None;
            }
        }
    })
}

fn shared_execution_hint(code: ErrorCode) -> Option<&'static str> {
    source_hint(code)
        .or_else(|| execution_hint(code))
        .or_else(|| provider_hint(code))
}

fn source_hint(code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::BinaryFileUnsupported => {
            "Use a reader for this file format or inspect its metadata with structureSearch; text and AST tools cannot read binary content."
        }
        ErrorCode::FileAccessFailed | ErrorCode::FileReadFailed => {
            "Verify the exact path, read permissions and text encoding; retry only after correcting the cause."
        }
        ErrorCode::ContentSecurityLimit | ErrorCode::SecurityValidationFailed => {
            "The source could not be processed safely. Choose a smaller supported source; do not bypass the security check."
        }
        ErrorCode::FileTooLarge | ErrorCode::LargeSourceWindowOnly => {
            "Locate a bounded region with localSearch, then read exact ranges with localFetch. Full-file structural analysis may remain unavailable."
        }
        ErrorCode::StaleSnapshot
        | ErrorCode::ResponseSnapshotExpired
        | ErrorCode::SourceMismatch => {
            "Use an emitted restart call, or restart the original request. Discard stale coordinates and never invent or edit a cursor."
        }
        ErrorCode::InvalidPagination | ErrorCode::UnsupportedResponsePagination => {
            "Copy the emitted continuation input unchanged. If it is no longer valid, restart the original request."
        }
        _ => return None,
    })
}

fn execution_hint(code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::ToolUnavailable | ErrorCode::UnroutedTool => {
            "Refresh tool discovery and check availability. Use an available tool that answers the same question, or report the missing capability."
        }
        ErrorCode::MissingConfiguration => {
            "Correct the missing configuration named in the error, then retry; report unavailable coverage until it is configured."
        }
        ErrorCode::CapabilityUnavailable
        | ErrorCode::SymbolsUnsupported
        | ErrorCode::LanguageUnsupported => {
            "Use localFetch for exact text or document outlines and localSearch for text candidates; these do not verify semantic identity."
        }
        ErrorCode::NotFound => {
            "Not found: verify the exact name, its repository or registry, the revision or version, and access rights; a miss here is not proof it exists nowhere else."
        }
        ErrorCode::VersionNotFound => {
            "Check the package name, registry and manifest/lockfile version; discover published versions before choosing another version."
        }
        ErrorCode::Busy | ErrorCode::DuplicateRequest => {
            "Let the active request finish before retrying; avoid launching duplicate work."
        }
        ErrorCode::Closed => {
            "Start a fresh runtime or MCP session before retrying the original request."
        }
        ErrorCode::ExecutionFailed
        | ErrorCode::OutputContractViolation
        | ErrorCode::Contract
        | ErrorCode::Response
        | ErrorCode::Runtime => {
            "Report the errorCode and failing call as a tool failure. Follow applicable recovery calls; do not infer absence or retry unchanged repeatedly."
        }
        ErrorCode::ScanFailed => {
            "Inspect the scan failure and access to its scope. Retry only after fixing the cause; report unscanned paths as a coverage gap."
        }
        _ => return None,
    })
}

fn provider_hint(code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::ProviderError
        | ErrorCode::ClassificationProviderError
        | ErrorCode::ClassificationProviderUnavailable => TRANSIENT_HINT,
        ErrorCode::ClassificationQuotaExhausted => {
            "Restore provider quota before retrying classification, or inspect the original sources without classification."
        }
        ErrorCode::InvalidClassificationConfiguration => {
            "Correct the classification provider configuration named in the error, or read the sources without classification."
        }
        ErrorCode::InvalidClassificationResponse => {
            "The provider returned an invalid classification. Inspect original sources; do not treat unclassified candidates as excluded."
        }
        ErrorCode::ClassificationContextEmpty
        | ErrorCode::ClassificationContextFailed
        | ErrorCode::ClassificationContextUnavailable
        | ErrorCode::CandidateIneligible => {
            "Inspect the source acquisition error and use an emitted source read. Unavailable or unjudged evidence is not irrelevant."
        }
        ErrorCode::ClassificationContextTooLarge | ErrorCode::ClassificationStateTooLarge => {
            "Narrow the classification source scope, or inspect bounded source ranges directly; preserve unknown candidates as a coverage gap."
        }
        ErrorCode::InvalidClassificationView => {
            "Choose a supported evidence view or inspect the source without classification."
        }
        ErrorCode::GitUnavailable => "Install or expose git on PATH before retrying the clone.",
        ErrorCode::CheckoutDirty => {
            "Preserve local changes in the checkout; use its current source or a separate checkout rather than overwriting it."
        }
        ErrorCode::CommitMismatch => {
            "Verify the requested revision and checkout commit; do not substitute a different revision as evidence."
        }
        ErrorCode::CacheUnavailable | ErrorCode::PersistentStorageDisabled => {
            "Check storage configuration and access; use GitHub read tools when a persistent local checkout is unavailable."
        }
        ErrorCode::GitFailed => {
            "Inspect the git error, repository access and revision; retry only after correcting the cause."
        }
        ErrorCode::WorkspaceRootInvalid => {
            "Choose an existing project directory as workspaceRoot and verify it contains the anchor file before retrying."
        }
        ErrorCode::DocumentSyncFailed | ErrorCode::RequestFailed | ErrorCode::ServerCrashed => {
            "Retry once only if retryable is true; otherwise inspect the related source read and report that semantic verification is unavailable."
        }
        ErrorCode::ServerUnavailable => {
            "Configure a language server for this file, or use related source/text reads and report the semantic coverage gap."
        }
        ErrorCode::Policy => {
            "Inspect the policy refusal and use an authorized source; do not work around denied access."
        }
        ErrorCode::NoMatches => {
            "Inspect the requested scope, filters and coverage; no matches in this scope do not establish repository-wide absence."
        }
        ErrorCode::AnchorUnresolved => {
            "Read the current source and locate the exact symbol and line before retrying; do not guess a different identity."
        }
        ErrorCode::InvalidGraphQuery => {
            "Inspect the graphQuery schema and correct the rejected operation or fields; do not retry the same query unchanged."
        }
        _ => return None,
    })
}
#[cfg(test)]
mod tests;
