use super::*;
use serde_json::json;

#[test]
fn error_stays_small_for_result_returns() {
    // `Result<T, Error>` is returned everywhere; RPC detail is boxed so the
    // error stays within clippy's `result_large_err` budget.
    assert!(
        std::mem::size_of::<Error>() <= 48,
        "{}",
        std::mem::size_of::<Error>()
    );
}

#[test]
fn error_code_round_trips_known_and_unknown_codes() {
    for code in [
        -32700, -32601, -32002, -32803, -32802, -32801, -32800, 7, -1,
    ] {
        assert_eq!(ErrorCode::from_i64(code).as_i64(), code);
    }
    assert_eq!(ErrorCode::from_i64(-32801), ErrorCode::ContentModified);
    assert_eq!(ErrorCode::from_i64(-328011), ErrorCode::Other(-328011));
}

#[test]
fn rpc_error_is_typed_and_keeps_an_informative_reason() {
    let error = Error::rpc(RpcError::from_value(json!({
        "code": -32801, "message": "content modified", "data": {"x": 1}
    })));
    assert_eq!(
        error.rpc_error().map(|error| error.code),
        Some(ErrorCode::ContentModified)
    );
    assert!(error.reason.contains("-32801"));
    assert!(error.reason.contains("content modified"));
    assert!(error.reason.contains("\"x\":1"));
    let cloned = error.clone();
    assert_eq!(cloned.kind(), error.kind());
}

#[test]
fn rpc_error_retry_policy_matches_by_code_not_text() {
    let modified = RpcError::from_value(json!({"code": -32801, "message": "m"}));
    assert!(modified.is_retryable());
    let cancelled = RpcError::from_value(json!({"code": -32802, "message": "m"}));
    assert!(
        !cancelled.is_retryable(),
        "ServerCancelled needs retriggerRequest"
    );
    let retrigger = RpcError::from_value(
        json!({"code": -32802, "message": "m", "data": {"retriggerRequest": true}}),
    );
    assert!(retrigger.is_retryable());
    let mentions =
        RpcError::from_value(json!({"code": -32603, "message": "code -32801: content modified"}));
    assert!(!mentions.is_retryable());
    let missing = RpcError::from_value(json!({"message": "no code"}));
    assert_eq!(missing.code, ErrorCode::InternalError);
    // A plain error whose text merely looks like an RPC error is not typed.
    let stringly = Error::new("LSP error: {\"code\":-32801}");
    assert_eq!(stringly.rpc_error().map(|error| error.code), None);
}
