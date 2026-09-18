use crate::providers::github::login::{get_token_with_refresh, refresh_auth_token_result};
use crate::providers::github::{
    StoredCredentials, delete_platform_credential, load_stored_credentials,
    store_platform_credential,
};
use crate::runtime::{HostOptions, RuntimeError, ToolRuntime};
use napi::{Env, bindgen_prelude::PromiseRaw};
use napi_derive::napi;
use serde_json::{Value, json};
use std::sync::Arc;

fn credential_error(error: crate::providers::github::ProviderError) -> napi::Error {
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

fn boundary_error(error: RuntimeError) -> napi::Error {
    let status = match error.code.as_str() {
        "invalidInput" | "securityValidationFailed" | "invalidCursor" | "staleCursor" => {
            napi::Status::InvalidArg
        }
        "cancelled" => napi::Status::Cancelled,
        _ => napi::Status::GenericFailure,
    };
    let reason = serde_json::to_string(&serde_json::json!({
        "kind": "octocode.nativeError",
        "code": error.code,
        "message": error.message,
        "payload": error.payload,
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
        let options: HostOptions = match options {
            Some(value) => serde_json::from_value(value).map_err(|_| {
                napi::Error::new(napi::Status::InvalidArg, "Invalid native host options")
            })?,
            None => HostOptions::default(),
        };
        Ok(Self {
            runtime: Arc::new(ToolRuntime::from_host(options).map_err(boundary_error)?),
        })
    }

    #[napi(getter)]
    pub fn abi_version(&self) -> u32 {
        crate::NATIVE_ABI_VERSION
    }
    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.runtime.requests.is_closed()
    }
    #[napi]
    pub fn catalog(&self) -> napi::Result<Value> {
        self.runtime.catalog().map_err(boundary_error)
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
        let credentials: StoredCredentials = serde_json::from_value(value).map_err(|_| {
            napi::Error::new(napi::Status::InvalidArg, "Invalid stored credentials")
        })?;
        store_platform_credential(&credentials).map_err(credential_error)?;
        Ok(json!({ "success": true }))
    }

    #[napi]
    pub fn get_credentials(&self, hostname: Option<String>) -> napi::Result<Value> {
        match load_stored_credentials(&default_hostname(hostname)).map_err(credential_error)? {
            Some(credentials) => serde_json::to_value(credentials).map_err(|_| {
                napi::Error::new(
                    napi::Status::GenericFailure,
                    "Failed to serialize stored credentials",
                )
            }),
            None => Ok(Value::Null),
        }
    }

    #[napi]
    pub fn delete_credentials(&self, hostname: Option<String>) -> napi::Result<Value> {
        delete_platform_credential(&default_hostname(hostname)).map_err(credential_error)?;
        Ok(json!({ "success": true }))
    }

    #[napi]
    pub async fn refresh_auth_token(&self, hostname: Option<String>) -> napi::Result<Value> {
        let result = refresh_auth_token_result(hostname.as_deref(), None).await;
        serde_json::to_value(result).map_err(|_| {
            napi::Error::new(
                napi::Status::GenericFailure,
                "Failed to serialize refresh result",
            )
        })
    }

    #[napi]
    pub async fn get_token_with_refresh(&self, hostname: Option<String>) -> napi::Result<Value> {
        let result = get_token_with_refresh(hostname.as_deref(), None).await;
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
