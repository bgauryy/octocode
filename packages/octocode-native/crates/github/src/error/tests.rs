use super::*;

/// Each kind's wire spelling is its contract `errorCode`, and it reads
/// back as the same kind.
#[test]
fn every_kind_serializes_as_its_error_code() {
    let expected = [
        "authentication",
        "permissionDenied",
        "notFound",
        "invalidInput",
        "rateLimited",
        "transport",
        "timeout",
        "cancelled",
        "responseTooLarge",
        "redirectDenied",
        "decode",
        "config",
        "credentialStoreUnavailable",
        "server",
        "unavailable",
        "httpStatus",
    ];
    for (kind, spelling) in ProviderErrorKind::ALL.into_iter().zip(expected) {
        let wire = serde_json::to_value(kind).expect("serialize");
        assert_eq!(wire, spelling, "{kind:?}");
        assert_eq!(wire, serde_json::to_value(kind.code()).expect("code"));
        assert_eq!(
            serde_json::from_value::<ProviderErrorKind>(wire).expect("deserialize"),
            kind
        );
    }
    assert!(serde_json::from_value::<ProviderErrorKind>("pathNotFound".into()).is_err());
}

/// A local filesystem failure reports the code the clone path reports
/// for the same cause, whatever kind carries it.
#[test]
fn a_file_access_reason_reports_file_access_failed() {
    let error = ProviderError::new(ProviderErrorKind::Configuration, "failed to write")
        .with_reason(ProviderErrorReason::FileAccess);
    assert_eq!(error.code(), ErrorCode::FileAccessFailed);
    assert!(!error.retryable);
    let plain = ProviderError::new(ProviderErrorKind::Configuration, "bad setting");
    assert_eq!(plain.code(), ErrorCode::Config);
}

#[test]
fn statuses_map_to_one_kind_each() {
    for (status, kind) in [
        (401, ProviderErrorKind::Authentication),
        (403, ProviderErrorKind::Permission),
        (404, ProviderErrorKind::NotFound),
        (410, ProviderErrorKind::NotFound),
        (400, ProviderErrorKind::Validation),
        (422, ProviderErrorKind::Validation),
        (429, ProviderErrorKind::RateLimited),
        (451, ProviderErrorKind::Unavailable),
        (500, ProviderErrorKind::Server),
        (503, ProviderErrorKind::Server),
        (599, ProviderErrorKind::Server),
        (405, ProviderErrorKind::HttpStatus),
        (408, ProviderErrorKind::HttpStatus),
        (304, ProviderErrorKind::HttpStatus),
    ] {
        assert_eq!(
            ProviderErrorKind::from_status(status),
            kind,
            "HTTP {status}"
        );
    }
}
