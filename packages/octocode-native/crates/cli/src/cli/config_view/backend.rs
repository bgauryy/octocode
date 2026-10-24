use super::{Failure, REQUEST_TIMEOUT};
use hyper::StatusCode;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_RESPONSE: u64 = 32 * 1024 * 1024;

fn unavailable() -> Failure {
    Failure::new(StatusCode::SERVICE_UNAVAILABLE, "UNAVAILABLE")
}

// Reuse the native management endpoint. A child can be stopped even while a
// filesystem operation blocks, and every request reads fresh configuration.
pub(super) async fn request(
    mut payload: Value,
    closing: &CancellationToken,
) -> Result<Value, Failure> {
    if closing.is_cancelled() {
        return Err(Failure::new(StatusCode::GONE, "CLOSED"));
    }
    let fingerprint = octocode_native::contracts::cli_presentation()
        .map_err(|_| unavailable())?
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?;
    payload["expectedFingerprint"] = Value::String(fingerprint.to_owned());
    let executable = std::env::current_exe().map_err(|_| unavailable())?;
    let mut child = Command::new(executable)
        .args(["config", "--manage"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| unavailable())?;
    let mut input = child.stdin.take().ok_or_else(unavailable)?;
    let output = child.stdout.take().ok_or_else(unavailable)?;
    let result = {
        let exchange = async {
            input
                .write_all(payload.to_string().as_bytes())
                .await
                .map_err(|_| unavailable())?;
            drop(input);
            let bytes = octocode_io::read::read_limited_async(output, MAX_RESPONSE)
                .await
                .map_err(|_| unavailable())?
                .ok_or_else(unavailable)?;
            let status = child.wait().await.map_err(|_| unavailable())?;
            let response: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
            if response["apiVersion"] != 1 {
                return Err(unavailable());
            }
            if response["success"] == false {
                let (status, code) = match response["error"]["code"].as_str() {
                    Some("CONFLICT") => (StatusCode::CONFLICT, "CONFLICT"),
                    Some("INVALID_INPUT") => (StatusCode::BAD_REQUEST, "INVALID_INPUT"),
                    Some("UNAVAILABLE") => (StatusCode::SERVICE_UNAVAILABLE, "UNAVAILABLE"),
                    _ => return Err(unavailable()),
                };
                let message = response["error"]["message"]
                    .as_str()
                    .ok_or_else(unavailable)?;
                return Err(Failure::safe(status, code, message.to_owned()));
            }
            if !status.success()
                || response["success"] != true
                || response["fingerprint"] != fingerprint
            {
                return Err(unavailable());
            }
            response.get("data").cloned().ok_or_else(unavailable)
        };
        tokio::select! {
            result = exchange => result,
            _ = closing.cancelled() => Err(Failure::new(StatusCode::GONE, "CLOSED")),
            _ = tokio::time::sleep(REQUEST_TIMEOUT) => Err(Failure::new(StatusCode::REQUEST_TIMEOUT, "TIMEOUT")),
        }
    };
    if result.is_err() {
        // Keep admission until the child has actually stopped.
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}
