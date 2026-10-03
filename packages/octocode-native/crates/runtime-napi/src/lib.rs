//! Node host adapter for the shared Octocode runtime.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

use napi::{Env, bindgen_prelude::PromiseRaw};
use napi_derive::napi;
use octocode_native::providers::github::login::{
    client_id_for_host, get_token_with_refresh_in_store, refresh_auth_token_result_in_store,
};
use octocode_native::providers::github::{CredentialStore, StoredCredentials};
use octocode_native::runtime::{HostOptions, RuntimeError, ToolRuntime};
use serde_json::{Value, json};
use std::sync::Arc;

fn credential_error(error: octocode_native::providers::github::ProviderError) -> napi::Error {
    napi::Error::new(napi::Status::GenericFailure, error.message.to_string())
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

/// Run the secret sanitizer over an error string. Error envelopes cross the
/// N-API boundary raw, so a secret echoed by a remote server (or embedded in a
/// provider payload) would otherwise leak. Sanitizing is a no-op unless a real
/// secret matches; a sanitizer panic fails closed to a redaction placeholder.
fn scrub_error_text(text: &str) -> String {
    octocode_engine::portable::sanitize_content(text, None)
        .map(|result| result.content)
        .unwrap_or_else(|_| "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned())
}

/// Sanitize every string leaf of an error payload in place.
fn scrub_error_payload(payload: &mut Value) {
    let _ = octocode_native::security::sanitize_json(payload, &mut |text: &str| {
        Ok::<_, std::convert::Infallible>(scrub_error_text(text))
    });
}

/// Contain a panic in a synchronous boundary method. napi (v3) does not wrap
/// synchronous `#[napi]` calls in `catch_unwind`, so an unguarded panic here
/// (deep in a dependency, or on pathological input) would unwind across the FFI
/// boundary and abort the entire host process. Converting it to a catchable
/// error mirrors `octocode_engine::portable::guard_panic`.
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
    /// The native pre-validation normalization for an MCP host that validates
    /// the canonical envelope itself: bare-query wrapping, a JSON-encoded
    /// `queries` array, JSON-encoded or bare-scalar list values, and lossless
    /// integer/boolean strings. Nothing is validated or defaulted.
    #[napi]
    pub fn normalize_input(&self, tool: String, input: Value) -> napi::Result<Value> {
        boundary_guard("normalize_input", || {
            Ok(octocode_native::contracts::normalize_envelope(&tool, input))
        })
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
        let admission = self.runtime.admit(request_id).map_err(boundary_error)?;
        let runtime = self.runtime.clone();
        env.spawn_future(async move {
            runtime
                .execute_admitted(admission, tool, input)
                .await
                .map(|outcome| outcome.structured_content)
                .map_err(boundary_error)
        })
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
        let admission = self.runtime.admit(request_id).map_err(boundary_error)?;
        let runtime = self.runtime.clone();
        env.spawn_future(async move {
            runtime
                .execute_mcp_admitted(admission, tool, input)
                .await
                .map_err(boundary_error)
        })
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
    fn error_message_secrets_are_redacted() {
        let token = format!("ghp_{}", "a".repeat(37));
        let scrubbed = scrub_error_text(&format!("upstream rejected token {token}"));
        assert!(
            !scrubbed.contains("ghp_"),
            "token leaked in error message: {scrubbed}"
        );
        assert!(scrubbed.contains("[REDACTED-"));
    }

    #[test]
    fn error_payload_secrets_are_redacted() {
        let token = format!("ghp_{}", "a".repeat(37));
        let mut payload = json!({"detail": format!("token={token}"), "nested": [{"note": token}]});
        scrub_error_payload(&mut payload);
        let serialized = serde_json::to_string(&payload).expect("serialize payload");
        assert!(
            !serialized.contains("ghp_"),
            "token leaked in error payload: {serialized}"
        );
    }
}
