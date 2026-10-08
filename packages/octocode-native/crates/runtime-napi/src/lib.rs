//! Node host adapter for the shared Octocode runtime.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

use napi::{Env, bindgen_prelude::PromiseRaw};
use napi_derive::napi;
use octocode_native::runtime::{HostOptions, RequestAdmission, RuntimeError, ToolRuntime};
use octocode_native::security::{scrub_error_payload, scrub_error_text};
use serde_json::Value;
use std::sync::Arc;

/// Admit `request_id` with its request options (`{githubToken?}`), then run
/// `call` on the runtime as a JS promise.
fn spawn_admitted<'env, Fut>(
    runtime: &Arc<ToolRuntime>,
    env: &'env Env,
    request_id: String,
    options: Option<Value>,
    call: impl FnOnce(Arc<ToolRuntime>, RequestAdmission) -> Fut,
) -> napi::Result<PromiseRaw<'env, Value>>
where
    Fut: std::future::Future<Output = Result<Value, RuntimeError>> + Send + 'static,
{
    // Admission runs synchronously in the `#[napi]` call: guard it like the
    // other sync boundary methods so a panic becomes a catchable error.
    let admission = boundary_guard("admission", || {
        runtime
            .admit_with(request_id, options)
            .map_err(boundary_error)
    })?;
    let future = call(Arc::clone(runtime), admission);
    env.spawn_future(async move { future.await.map_err(boundary_error) })
}

#[napi]
pub struct NativeRuntime {
    runtime: Arc<ToolRuntime>,
}

/// Contain a panic in a synchronous boundary method. napi (v3) does not wrap
/// synchronous `#[napi]` calls in `catch_unwind`, so an unguarded panic here
/// (deep in a dependency, or on pathological input) would unwind across the FFI
/// boundary and abort the entire host process; it becomes a catchable error.
fn boundary_guard<T>(what: &str, call: impl FnOnce() -> napi::Result<T>) -> napi::Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).unwrap_or_else(|_| {
        Err(napi::Error::new(
            napi::Status::GenericFailure,
            format!("{what} failed on pathological input"),
        ))
    })
}

fn boundary_error(error: RuntimeError) -> napi::Error {
    let status = match error.code.as_str() {
        "invalidInput" | "securityValidationFailed" | "invalidCursor" | "staleCursor" => {
            napi::Status::InvalidArg
        }
        "cancelled" => napi::Status::Cancelled,
        _ => napi::Status::GenericFailure,
    };
    let message = scrub_error_text(&error.message);
    let payload = error.payload.map(|payload| {
        let mut payload = *payload;
        scrub_error_payload(&mut payload);
        payload
    });
    let reason = serde_json::to_string(&serde_json::json!({
        "kind": "octocode.nativeError",
        "code": error.code,
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
    #[napi(constructor)]
    pub fn new(options: Option<Value>) -> napi::Result<Self> {
        boundary_guard("NativeRuntime::new", || {
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

    #[napi(getter)]
    pub fn abi_version(&self) -> u32 {
        octocode_native::NATIVE_ABI_VERSION
    }
    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.runtime.requests.is_closed()
    }
    #[napi]
    pub fn catalog(&self) -> napi::Result<Value> {
        boundary_guard("catalog", || self.runtime.catalog().map_err(boundary_error))
    }
    #[napi]
    pub fn execute<'env>(
        &self,
        env: &'env Env,
        request_id: String,
        tool: String,
        input: Value,
        options: Option<Value>,
    ) -> napi::Result<PromiseRaw<'env, Value>> {
        spawn_admitted(
            &self.runtime,
            env,
            request_id,
            options,
            |runtime, admission| async move {
                runtime
                    .execute_admitted(admission, tool, input)
                    .await
                    .map(|outcome| outcome.structured_content)
            },
        )
    }
    #[napi]
    pub fn cancel(&self, request_id: String) -> bool {
        self.runtime.requests.cancel(&request_id)
    }
    #[napi]
    pub fn execute_mcp<'env>(
        &self,
        env: &'env Env,
        request_id: String,
        tool: String,
        input: Value,
        options: Option<Value>,
    ) -> napi::Result<PromiseRaw<'env, Value>> {
        spawn_admitted(
            &self.runtime,
            env,
            request_id,
            options,
            |runtime, admission| async move {
                runtime.execute_mcp_admitted(admission, tool, input).await
            },
        )
    }
    /// Startup clasify check; a failed probe makes clasify unavailable in
    /// this runtime's catalog. Hosts call it before `catalog()`.
    #[napi]
    pub async fn probe_classification(&self) -> Value {
        let mut probe = self.runtime.probe_classification().await;
        if let Some(message) = probe.get_mut("message")
            && let Some(text) = message.as_str()
        {
            *message = Value::String(scrub_error_text(text));
        }
        probe
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
            code: "providerError".into(),
            message: format!("upstream rejected token {token}"),
            payload: Some(Box::new(
                json!({"detail": format!("token={token}"), "nested": [{"note": token}]}),
            )),
            validation_issues: None,
        });
        assert!(!error.reason.contains("ghp_"), "{}", error.reason);
        assert!(error.reason.contains("[REDACTED-"), "{}", error.reason);
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
}
