use crate::runtime::{HostOptions, RuntimeError, ToolRuntime};
use napi_derive::napi;
use serde_json::Value;

#[napi]
pub struct NativeRuntime {
    runtime: ToolRuntime,
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
            runtime: ToolRuntime::from_host(options).map_err(boundary_error)?,
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
    pub async fn execute(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> napi::Result<Value> {
        self.runtime
            .execute(request_id, tool, input)
            .await
            .map(|outcome| outcome.structured_content)
            .map_err(boundary_error)
    }
    #[napi]
    pub fn cancel(&self, request_id: String) -> bool {
        self.runtime.requests.cancel(&request_id)
    }
    #[napi]
    pub async fn execute_mcp(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> napi::Result<Value> {
        self.runtime
            .execute_mcp(request_id, tool, input)
            .await
            .map_err(boundary_error)
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
