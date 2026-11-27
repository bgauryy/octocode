use super::*;
use octocode_github::ProviderErrorKind;

/// A network failure names the proxy the request went through, so a
/// proxy that refuses GitHub is told from GitHub being down; a direct
/// request names none.
#[test]
fn a_network_failure_names_the_proxy_in_effect() {
    let mut error = ProviderError::new(ProviderErrorKind::Transport, "GitHub transport failed");
    assert_eq!(provider_message(&error), "Network connection failed");
    error.proxy = Some("http://proxy.internal:3128".into());
    let message = provider_message(&error);
    assert!(message.contains("http://proxy.internal:3128"), "{message}");
    assert!(
        message.starts_with("Network connection failed"),
        "{message}"
    );
}
