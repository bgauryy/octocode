//! Node host adapter for the shared Octocode runtime.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

use napi::{Env, bindgen_prelude::PromiseRaw};
use napi_derive::napi;
use octocode_native::providers::github::login::{
    client_id_for_host, get_token_with_refresh_in_store, refresh_auth_token_result_in_store,
};
use octocode_native::providers::github::{CredentialStore, StoredCredentials};
use octocode_native::runtime::{HostOptions, RequestAdmission, RuntimeError, ToolRuntime};
use octocode_native::security::{scrub_error_payload, scrub_error_text};
use serde_json::{Value, json};
use std::sync::Arc;

fn credential_error(error: octocode_native::providers::github::ProviderError) -> napi::Error {
    napi::Error::new(napi::Status::GenericFailure, error.message.to_string())
}

/// Admit `request_id`, then run `call` on the runtime as a JS promise.
fn spawn_admitted<'env, Fut>(
    runtime: &Arc<ToolRuntime>,
    env: &'env Env,
    request_id: String,
    call: impl FnOnce(Arc<ToolRuntime>, RequestAdmission) -> Fut,
) -> napi::Result<PromiseRaw<'env, Value>>
where
    Fut: std::future::Future<Output = Result<Value, RuntimeError>> + Send + 'static,
{
    let admission = runtime.admit(request_id).map_err(boundary_error)?;
    let future = call(Arc::clone(runtime), admission);
    env.spawn_future(async move { future.await.map_err(boundary_error) })
}

fn default_hostname(hostname: Option<String>) -> String {
    hostname
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "github.com".into())
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
    ) -> napi::Result<PromiseRaw<'env, Value>> {
        spawn_admitted(
            &self.runtime,
            env,
            request_id,
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
    ) -> napi::Result<PromiseRaw<'env, Value>> {
        spawn_admitted(
            &self.runtime,
            env,
            request_id,
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

    #[napi]
    pub fn store_credentials(&self, value: Value) -> napi::Result<Value> {
        boundary_guard("store_credentials", || {
            let credentials: StoredCredentials = serde_json::from_value(value).map_err(|_| {
                napi::Error::new(napi::Status::InvalidArg, "Invalid stored credentials")
            })?;
            CredentialStore::new(&self.runtime.config().home)
                .save(&credentials)
                .map_err(credential_error)?;
            Ok(json!({ "success": true }))
        })
    }

    #[napi]
    pub fn get_credentials(&self, hostname: Option<String>) -> napi::Result<Value> {
        boundary_guard("get_credentials", || {
            match CredentialStore::new(&self.runtime.config().home)
                .load(&default_hostname(hostname))
                .map_err(credential_error)?
            {
                Some((credentials, _)) => serde_json::to_value(credentials).map_err(|_| {
                    napi::Error::new(
                        napi::Status::GenericFailure,
                        "Failed to serialize stored credentials",
                    )
                }),
                None => Ok(Value::Null),
            }
        })
    }

    #[napi]
    pub fn delete_credentials(&self, hostname: Option<String>) -> napi::Result<Value> {
        boundary_guard("delete_credentials", || {
            CredentialStore::new(&self.runtime.config().home)
                .delete(&default_hostname(hostname))
                .map_err(credential_error)?;
            Ok(json!({ "success": true }))
        })
    }

    #[napi]
    pub async fn refresh_auth_token(&self, hostname: Option<String>) -> napi::Result<Value> {
        let store = CredentialStore::new(&self.runtime.config().home);
        let host = default_hostname(hostname);
        let client = client_id_for_host(
            &host,
            self.runtime.config().env_value("OCTOCODE_GITHUB_CLIENT_ID"),
        );
        let result = refresh_auth_token_result_in_store(&host, client, &store).await;
        serde_json::to_value(result).map_err(|_| {
            napi::Error::new(
                napi::Status::GenericFailure,
                "Failed to serialize refresh result",
            )
        })
    }

    #[napi]
    pub async fn get_token_with_refresh(&self, hostname: Option<String>) -> napi::Result<Value> {
        let store = CredentialStore::new(&self.runtime.config().home);
        let client = self.runtime.config().env_value("OCTOCODE_GITHUB_CLIENT_ID");
        let result = get_token_with_refresh_in_store(hostname.as_deref(), client, &store).await;
        serde_json::to_value(result).map_err(|_| {
            napi::Error::new(
                napi::Status::GenericFailure,
                "Failed to serialize token refresh result",
            )
        })
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
}
