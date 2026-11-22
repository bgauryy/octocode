use crate::response::output::{SANDBOX_HINT, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

pub(crate) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::response::result::ToolDiagnostics,
) {
    let Some(data) = row.get_mut("data") else {
        return;
    };
    if let Some(fields) = data.as_object_mut() {
        for reason in diagnostics.partial_reasons {
            crate::response::paging::mark_partial(fields, reason, &[]);
        }
    }
    crate::response::result::add_hints(data, diagnostics.hints);
}

/// Authentication recovery shared by every GitHub tool.
pub const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// The one recovery for a cancelled call, on every tool and stage.
pub const CANCELLED_HINT: &str = "Retry only if the result is still needed and what cancelled it changed; otherwise report the cancellation.";

/// The recovery of a source too large to read or analyse whole: valid only
/// where localSearch and localFetch ranges still serve it.
pub const LARGE_SOURCE_HINT: &str = "Locate a bounded region with localSearch, then read it with localFetch: ranges for many lines, matchString with a rare literal for a few huge (minified) lines. Full-file structural analysis may remain unavailable.";

/// The recovery of a `fileAccessFailed` whose failing operation was a write
/// under the home (a graph snapshot, a clone, a materialized tree): the
/// producer that writes attaches it, so a read's own hint (which names read
/// permissions and text encoding) never answers a full or read-only home.
pub const WRITE_ACCESS_HINT: &str = "Writing under the Octocode home failed. Point OCTOCODE_HOME (graphs: OCTOCODE_GRAPH_HOME) at a writable folder with free disk space, or free space there, then repeat the same call.";

const TRANSIENT_HINT: &str = "Retry once only if retryable is true; if it persists, check provider availability and report the coverage gap.";

/// The one finaliser of an error row's class: states `errorClass` from the
/// row's `errorCode`, and on a transient row a boolean `retryable` (the
/// emitter's verdict when it gave one, else true; a cancelled call cannot be
/// retried). Any other class carries no `retryable`. Idempotent.
pub fn finish_error_row(data: &mut Value) {
    use crate::id::error_codes::{self, ErrorClass};
    let Some(code) = crate::id::error_code_of(data) else {
        return;
    };
    let Some(fields) = data.as_object_mut() else {
        return;
    };
    let class = error_codes::class(code);
    fields.insert("errorClass".into(), json!(class));
    if class == ErrorClass::Transient {
        if !fields.get("retryable").is_some_and(Value::is_boolean) {
            fields.insert(
                "retryable".into(),
                Value::from(code != ErrorCode::Cancelled),
            );
        }
    } else {
        fields.remove("retryable");
    }
}

/// Fill missing guidance without replacing producer-specific recovery or
/// repeating it, in the row's `hints` now: for a row the stage places no
/// notices on (one created after placement, a rejected call's envelope).
/// Call before the output security gate so query-derived facts and future
/// tool-owned guidance share the normal redaction boundary.
pub fn attach_error_hint(tool: &dyn ToolOutput, data: &mut Value, query: &Value) {
    if let Some(hint) = missing_error_hint(tool, data, query) {
        crate::response::result::add_hints(data, [hint]);
    }
}

/// [`attach_error_hint`] for a row the stage is still to shape: the shared
/// recovery is a static note, so it is a notice, which the stage states
/// once per response, on the first failed row that needs it
/// ([`crate::response::notices::place`]). A producer's own hints stay on
/// their row.
pub fn note_error_hint(tool: &dyn ToolOutput, data: &mut Value, query: &Value) {
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
    // A notice already words the row's recovery (a missing repository's
    // caution, said once per response).
    if data
        .get(crate::contracts::envelope::NOTICES)
        .and_then(Value::as_array)
        .is_some_and(|notices| !notices.is_empty())
    {
        return None;
    }
    // A row whose `related` call carries the fix needs no shared sentence
    // beside it: the error says what failed and the call is what to run.
    if data
        .get("related")
        .and_then(Value::as_array)
        .is_some_and(|calls| !calls.is_empty())
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
pub fn error_code_hint(
    tool: &dyn ToolOutput,
    code: ErrorCode,
    query: &Value,
) -> Option<&'static str> {
    Some(match code {
        ErrorCode::OutsideAllowedRoots | ErrorCode::SymlinkEscape => SANDBOX_HINT,
        ErrorCode::Authentication => GITHUB_AUTH_RECOVERY_HINT,
        ErrorCode::PermissionDenied => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        // Names no wait value: `rateLimit`, when the provider stated one,
        // carries it on the row.
        ErrorCode::RateLimited | ErrorCode::ClassificationRateLimited => {
            "Wait for the provider's limit to reset before retrying."
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
        ErrorCode::Config => {
            "Inspect octocode config --json and correct the setting named in the error before retrying."
        }
        ErrorCode::CredentialStoreUnavailable => {
            "Restore access to the credential store or supply the provider token through its supported environment setting."
        }
        // The provider withholds the resource for a reason the message
        // states (a legal block, or a link check its git tree could not
        // complete), and the same call meets the same refusal.
        ErrorCode::Unavailable => {
            "The provider did not serve this resource and the error states why; retrying the same call will not help. Read a different path or ref, or report the resource as unavailable."
        }
        ErrorCode::HttpStatus => {
            "Check httpStatus and the message; this status is not a network failure."
        }
        ErrorCode::PathPolicyDenied => crate::policy::discovery::WITHHELD_HINT,
        ErrorCode::InvalidGlob => "Correct the glob named in the error; globs select paths, while regexEngine selects text-pattern syntax. Do not change the intended scope.",
        code => {
            if let Some(hint) = tool.error_hint(code, query) {
                hint
            } else {
                shared_execution_hint(code)?
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
        ErrorCode::FileTooLarge | ErrorCode::LargeSourceWindowOnly => LARGE_SOURCE_HINT,
        // The message states the recovery (run next.restart, or rerun the
        // original request without cursor) and the server instructions say
        // to drop the old pages: a hint would only repeat them.
        ErrorCode::StaleSnapshot | ErrorCode::ResponseSnapshotExpired => {
            crate::response::output::NO_HINT
        }
        ErrorCode::SourceMismatch => {
            "Restart the original request without its cursor; never invent or edit a cursor."
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
        ErrorCode::CapabilityUnavailable | ErrorCode::LanguageUnsupported => {
            "Use localFetch for exact text or document outlines and localSearch for text candidates; these do not verify semantic identity."
        }
        // One text for any resource, on every tool's `notFound` row that does
        // not state a recovery of its own. That a miss is no absence
        // elsewhere is in the server instructions.
        ErrorCode::NotFound => crate::response::output::NOT_FOUND_HINT,
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
            "The refusal follows the user's configuration: use an authorized source, and leave any opt-in it names to the user."
        }
        ErrorCode::InvalidGraphQuery => {
            "Inspect the graphQuery schema and correct the rejected operation or fields; do not retry the same query unchanged."
        }
        _ => return None,
    })
}
