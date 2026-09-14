use crate::providers::github::{
    ProviderError, delete_credentials_value, get_credentials_value, get_token_with_refresh_value,
    refresh_auth_token_value, store_credentials_value,
};
use crate::runtime::{HostOptions, RuntimeError, ToolRuntime};
use napi::{Env, bindgen_prelude::PromiseRaw};
use napi_derive::napi;
use serde_json::Value;
use std::sync::Arc;

#[napi]
pub struct NativeRuntime {
    runtime: Arc<ToolRuntime>,
}

fn credential_boundary_error(error: ProviderError) -> napi::Error {
    let status = match error.kind {
        crate::providers::github::ProviderErrorKind::Validation => napi::Status::InvalidArg,
        _ => napi::Status::GenericFailure,
    };
    let reason = serde_json::to_string(&serde_json::json!({
        "kind": "octocode.nativeError",
        "code": error.kind,
        "message": error.message,
        "status": error.status,
    }))
    .unwrap_or_else(|_| {
        "{\"kind\":\"octocode.nativeError\",\"code\":\"serializationFailed\",\"message\":\"Failed to serialize native error\"}".into()
    });
    napi::Error::new(status, reason)
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
        store_credentials_value(value).map_err(credential_boundary_error)
    }

    #[napi]
    pub fn get_credentials(&self, hostname: Option<String>) -> napi::Result<Value> {
        get_credentials_value(hostname.as_deref()).map_err(credential_boundary_error)
    }

    #[napi]
    pub fn delete_credentials(&self, hostname: Option<String>) -> napi::Result<Value> {
        delete_credentials_value(hostname.as_deref()).map_err(credential_boundary_error)
    }

    #[napi]
    pub async fn refresh_auth_token(&self, hostname: Option<String>) -> napi::Result<Value> {
        refresh_auth_token_value(hostname.as_deref())
            .await
            .map_err(credential_boundary_error)
    }

    #[napi]
    pub async fn get_token_with_refresh(&self, hostname: Option<String>) -> napi::Result<Value> {
        get_token_with_refresh_value(hostname.as_deref())
            .await
            .map_err(credential_boundary_error)
    }
}

impl Drop for NativeRuntime {
    fn drop(&mut self) {
        self.runtime.begin_close();
    }
}
