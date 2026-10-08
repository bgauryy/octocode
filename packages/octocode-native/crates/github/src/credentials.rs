//! Credentials supplied by the host; acquisition and storage stay with the host.
use secrecy::{ExposeSecret, SecretString};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    Override,
    Environment,
    Storage,
    Home,
    GhCli,
}

#[derive(Clone)]
pub struct ResolvedCredential {
    secret: SecretString,
    pub source: CredentialSource,
}

impl ResolvedCredential {
    pub fn new(secret: impl Into<SecretString>, source: CredentialSource) -> Self {
        Self {
            secret: secret.into(),
            source,
        }
    }
    /// Deliberately expose the token for an authenticated operation. Debug output stays redacted.
    pub fn expose_secret(&self) -> &str {
        self.secret.expose_secret()
    }
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("secret", &"[REDACTED]")
            .field("source", &self.source)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_secret_access_does_not_expose_debug_output() {
        let credential = ResolvedCredential::new("private-token", CredentialSource::Override);
        assert_eq!(credential.expose_secret(), "private-token");
        let debug = format!("{credential:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("private-token"));
    }
}
