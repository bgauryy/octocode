//! Saved-response access: one successful credential identity per batch.
use super::*;

#[test]
fn saved_response_access_requires_one_successful_credential_identity() {
    let access = ResponseAccess::default();
    assert_eq!(access.identity(), Ok(None));
    let request = |token: &str| {
        Ok(RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1024),
            Some(ResolvedCredential::new(token, CredentialSource::Home)),
        ))
    };
    access.observe("github.com", &request("one"));
    let first = access.identity().expect("one identity");
    assert!(first.is_some());
    access.observe("github.com", &request("one"));
    assert_eq!(access.identity(), Ok(first));
    access.observe("github.com", &request("two"));
    assert_eq!(access.identity(), Err(()));
    let failed = ResponseAccess::default();
    failed.observe(
        "github.com",
        &Err(ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "fixture",
        )),
    );
    failed.observe("github.com", &request("one"));
    assert_eq!(failed.identity(), Err(()));
}
