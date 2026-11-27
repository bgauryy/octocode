use super::*;

fn row(code: ErrorCode, status: u16) -> Value {
    DomainResult::provider(
        code,
        "provider said no",
        Vec::new(),
        None,
        Upstream {
            http_status: Some(status),
            ..Upstream::default()
        },
    )
    .data
}

#[test]
fn a_status_the_error_code_names_is_not_repeated() {
    let missing = row(ErrorCode::NotFound, 404);
    assert_eq!(missing["errorCode"], "notFound");
    assert!(missing.get("httpStatus").is_none(), "{missing}");
}

#[test]
fn a_status_the_error_code_does_not_name_is_stated() {
    let gateway = row(ErrorCode::Server, 502);
    assert_eq!(gateway["errorCode"], "server");
    assert_eq!(gateway["httpStatus"], 502);
    let teapot = row(ErrorCode::HttpStatus, 418);
    assert_eq!(teapot["errorCode"], "httpStatus");
    assert_eq!(teapot["httpStatus"], 418);
    let limited = row(ErrorCode::RateLimited, 403);
    assert_eq!(limited["httpStatus"], 403, "{limited}");
}
