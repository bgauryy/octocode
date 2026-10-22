//! Node host adapter for the shared Octocode runtime.
// Test-compiled code may assert with unwrap/expect/panic. A crate attribute,
// not clippy.toml's `allow-*-in-tests`: clippy does not treat modules mounted
// under `cfg(all(test, unix))` as test code.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

use napi::{Env, bindgen_prelude::PromiseRaw};
use napi_derive::napi;
use octocode_contract_types::ErrorCode;
use octocode_native::id::error_codes::{self, ErrorClass};
use octocode_native::runtime::error::mcp_result;
use octocode_native::runtime::{Channel, HostOptions, RuntimeError, ToolRuntime};
use octocode_native::security::{scrub_error_payload, scrub_error_text};
use serde_json::Value;
use std::sync::Arc;

#[napi]
pub struct NativeRuntime {
    runtime: Arc<ToolRuntime>,
}

/// A rejection JS receives: the contract's class of the code decides the
/// status (a caller error, a cancellation, or a failure).
fn boundary_error(error: RuntimeError) -> napi::Error {
    let status = match (error.code, error_codes::class(error.code)) {
        (ErrorCode::Cancelled, _) => napi::Status::Cancelled,
        (_, ErrorClass::InvalidInput) => napi::Status::InvalidArg,
        _ => napi::Status::GenericFailure,
    };
    envelope(
        status,
        error_codes::as_str(error.code),
        &error.message,
        error.payload.map(|payload| *payload),
    )
}

/// Run a synchronous boundary call; a panic becomes the same scrubbed
/// `octocode.nativeError` envelope as any other failure. `#[napi(catch_unwind)]`
/// stays on each entry point as the backstop that keeps the host alive.
fn boundary<T>(call: impl FnOnce() -> napi::Result<T>) -> napi::Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "native call panicked".to_owned());
        Err(envelope(
            napi::Status::GenericFailure,
            "internalError",
            &message,
            None,
        ))
    })
}

/// The JSON error envelope JS receives; the message and payload are scrubbed.
fn envelope(
    status: napi::Status,
    code: &str,
    message: &str,
    payload: Option<Value>,
) -> napi::Error {
    let message = scrub_error_text(message);
    let payload = payload.map(|mut payload| {
        scrub_error_payload(&mut payload);
        payload
    });
    let reason = serde_json::to_string(&serde_json::json!({
        "kind": "octocode.nativeError",
        "code": code,
        "message": message,
        "payload": payload,
    }))
    .unwrap_or_else(|_| {
        "{\"kind\":\"octocode.nativeError\",\"code\":\"serializationFailed\",\"message\":\"Failed to serialize native error\"}".into()
    });
    napi::Error::new(status, reason)
}

#[napi]
impl NativeRuntime {
    #[napi(constructor, catch_unwind)]
    pub fn new(options: Option<Value>) -> napi::Result<Self> {
        boundary(|| {
            let options: HostOptions = match options {
                Some(value) => serde_json::from_value(value).map_err(|_| {
                    napi::Error::new(napi::Status::InvalidArg, "Invalid native host options")
                })?,
                None => HostOptions::default(),
            };
            Ok(Self {
                runtime: Arc::new(ToolRuntime::from_host(options).map_err(boundary_error)?),
            })
        })
    }

    #[napi(getter, catch_unwind)]
    pub fn abi_version(&self) -> napi::Result<u32> {
        boundary(|| Ok(octocode_native::NATIVE_ABI_VERSION))
    }
    #[napi(getter, catch_unwind)]
    pub fn closed(&self) -> napi::Result<bool> {
        boundary(|| Ok(self.runtime.requests.is_closed()))
    }
    #[napi(catch_unwind)]
    pub fn catalog(&self) -> napi::Result<Value> {
        boundary(|| Ok(self.runtime.catalog()))
    }
    #[napi(catch_unwind)]
    pub fn cancel(&self, request_id: String) -> napi::Result<bool> {
        boundary(|| Ok(self.runtime.requests.cancel(&request_id)))
    }
    /// One MCP call: the `CallToolResult`, a rejected call included (a
    /// typed tool error). Only a cancelled request or a closing runtime
    /// rejects. Admission (with its request options, `{githubToken?}`) runs
    /// before the promise exists, so a later `cancel` finds the request.
    #[napi(catch_unwind)]
    pub fn execute_mcp<'env>(
        &self,
        env: &'env Env,
        request_id: String,
        tool: String,
        input: Value,
        options: Option<Value>,
    ) -> napi::Result<PromiseRaw<'env, Value>> {
        boundary(|| {
            let admission = self.runtime.admit_with(request_id, options);
            let runtime = Arc::clone(&self.runtime);
            env.spawn_future(async move {
                let result = match admission {
                    Ok(admission) => {
                        runtime
                            .execute(admission, tool.clone(), input, Channel::Mcp)
                            .await
                    }
                    Err(error) => Err(error),
                };
                mcp_result(&tool, result).map_err(boundary_error)
            })
        })
    }
    #[napi]
    pub async fn close(&self) {
        self.runtime.close().await;
    }
}

impl Drop for NativeRuntime {
    fn drop(&mut self) {
        self.runtime.begin_close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boundary_errors_redact_secrets_in_message_and_payload() {
        let token = format!("ghp_{}", "a".repeat(37));
        let error = boundary_error(RuntimeError {
            code: ErrorCode::ProviderError,
            message: format!("upstream rejected token {token}"),
            payload: Some(Box::new(
                json!({"detail": format!("token={token}"), "nested": [{"note": token}]}),
            )),
            validation_issues: None,
        });
        assert!(!error.reason.contains("ghp_"), "{}", error.reason);
        assert!(error.reason.contains("[REDACTED-"), "{}", error.reason);
    }
    /// The contract's class of a code decides the rejection status; no
    /// interface keeps its own code list.
    #[test]
    fn rejection_status_follows_the_declared_error_class() {
        for (code, status) in [
            (ErrorCode::InvalidInput, napi::Status::InvalidArg),
            (ErrorCode::PathPolicyDenied, napi::Status::InvalidArg),
            (ErrorCode::Cancelled, napi::Status::Cancelled),
            (
                ErrorCode::SecurityValidationFailed,
                napi::Status::GenericFailure,
            ),
            (ErrorCode::Closed, napi::Status::GenericFailure),
        ] {
            let error = boundary_error(RuntimeError {
                code,
                message: "m".into(),
                payload: None,
                validation_issues: None,
            });
            assert_eq!(error.status, status, "{code}");
        }
    }

    /// M4: a host `timeoutSecs` past the runtime's deadline range is a
    /// catchable constructor error, never a later panic in sync `execute`.
    #[test]
    fn oversized_timeout_secs_is_a_constructor_error() {
        let built = NativeRuntime::new(Some(
            json!({"timeoutSecs": u64::MAX, "env": {"OCTOCODE_HOME": std::env::temp_dir()}}),
        ));
        let Err(error) = built else {
            panic!("an unrepresentable timeout must be rejected");
        };
        assert!(error.reason.contains("timeoutSecs"), "{}", error.reason);
    }

    /// A host over its own home, so no test reads or writes the user's.
    fn host(name: &str) -> NativeRuntime {
        let home =
            std::env::temp_dir().join(format!("octocode-napi-{name}-{}", std::process::id()));
        NativeRuntime::new(Some(json!({"env": {"OCTOCODE_HOME": home}}))).expect("native host")
    }

    #[test]
    fn invalid_host_options_are_an_invalid_argument_that_echoes_nothing() {
        for options in [
            json!({"unknownOption": true}),
            json!({"timeoutSecs": "ten"}),
            json!({"env": {"OCTOCODE_HOME": 7}}),
            json!("not an object"),
        ] {
            let Err(error) = NativeRuntime::new(Some(options.clone())) else {
                panic!("{options} must be rejected");
            };
            assert_eq!(error.status, napi::Status::InvalidArg, "{options}");
            assert_eq!(error.reason, "Invalid native host options", "{options}");
        }
    }

    #[test]
    fn a_host_reports_its_abi_and_an_open_catalog() {
        let native = host("catalog");
        assert_eq!(
            native.abi_version().expect("abi"),
            octocode_native::NATIVE_ABI_VERSION
        );
        assert!(!native.closed().expect("closed"));
        let catalog = native.catalog().expect("catalog");
        for key in [
            "fingerprint",
            "tools",
            "lspServers",
            "presentation",
            "grammarCapabilities",
        ] {
            assert!(catalog.get(key).is_some(), "catalog lacks {key}");
        }
        assert!(
            catalog["fingerprint"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        );
        let tools = catalog["tools"].as_array().expect("tools");
        assert!(!tools.is_empty());
        assert!(
            tools
                .iter()
                .all(|tool| tool["name"].is_string() && tool["available"].is_boolean()),
            "{catalog}"
        );
    }

    #[test]
    fn cancel_reaches_only_a_request_that_is_in_flight() {
        let native = host("cancel");
        assert!(!native.cancel("never-admitted".into()).expect("cancel"));
        let admission = native.runtime.admit("r1".into()).expect("admit");
        assert!(
            !native.cancel("r2".into()).expect("cancel"),
            "another id is untouched"
        );
        assert!(native.cancel("r1".into()).expect("cancel"));
        drop(admission);
        assert!(
            !native.cancel("r1".into()).expect("cancel"),
            "a finished request is no longer cancellable"
        );
    }

    #[test]
    fn dropping_the_handle_closes_the_runtime_and_cancels_what_is_running() {
        let native = host("drop");
        let runtime = Arc::clone(&native.runtime);
        let admission = runtime.admit("running".into()).expect("admit");
        assert!(!runtime.requests.is_closed());
        drop(native);
        assert!(
            runtime.requests.is_closed(),
            "the host handle's drop begins close"
        );
        assert!(
            runtime.admit("late".into()).is_err(),
            "a closing runtime admits nothing new"
        );
        drop(admission);
    }

    #[test]
    fn the_error_envelope_is_one_json_document_with_a_null_payload_by_default() {
        let error = envelope(
            napi::Status::GenericFailure,
            "closed",
            "shutting down",
            None,
        );
        let envelope: Value = serde_json::from_str(&error.reason).expect("JSON envelope");
        assert_eq!(
            envelope,
            json!({
                "kind": "octocode.nativeError",
                "code": "closed",
                "message": "shutting down",
                "payload": null
            })
        );
        assert_eq!(error.status, napi::Status::GenericFailure);
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::boundary;

    #[test]
    fn a_panic_returns_the_scrubbed_native_error_envelope() {
        let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        let error = boundary(|| -> napi::Result<()> { panic!("failed with {secret}") })
            .expect_err("a panic is an error, not an abort");
        let envelope: serde_json::Value =
            serde_json::from_str(&error.reason).expect("JSON envelope");
        assert_eq!(envelope["kind"], "octocode.nativeError");
        assert_eq!(envelope["code"], "internalError");
        assert!(!error.reason.contains(secret), "{}", error.reason);
    }

    #[test]
    fn a_normal_result_passes_through() {
        assert_eq!(boundary(|| Ok::<_, napi::Error>(7)).expect("ok"), 7);
    }
}
