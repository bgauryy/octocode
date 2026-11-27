use super::*;

#[test]
fn explicit_secret_access_does_not_expose_debug_output() {
    let credential = ResolvedCredential::new("private-token", CredentialSource::Override);
    assert_eq!(credential.expose_secret(), "private-token");
    let debug = format!("{credential:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("private-token"));
}
