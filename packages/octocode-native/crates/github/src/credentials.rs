//! Credentials supplied by the host; acquisition and storage stay with the host.
use std::{future::Future, pin::Pin};

use secrecy::{ExposeSecret, SecretString};

use super::ProviderError;

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

#[derive(Clone)]
pub struct CredentialRequest<'a> {
    pub host: &'a str,
    pub override_token: Option<&'a str>,
}

pub trait CredentialResolver: Send + Sync {
    fn resolve<'a>(
        &'a self,
        request: CredentialRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ResolvedCredential>, ProviderError>> + Send + 'a>>;
}

#[derive(Clone, Default)]
pub struct StaticCredentialResolver {
    token: Option<ResolvedCredential>,
}

impl StaticCredentialResolver {
    pub fn anonymous() -> Self {
        Self::default()
    }
    pub fn new(token: impl Into<SecretString>, source: CredentialSource) -> Self {
        Self {
            token: Some(ResolvedCredential::new(token, source)),
        }
    }
}

impl CredentialResolver for StaticCredentialResolver {
    fn resolve<'a>(
        &'a self,
        request: CredentialRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ResolvedCredential>, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            if let Some(token) = request.override_token {
                return Ok(Some(ResolvedCredential::new(
                    token,
                    CredentialSource::Override,
                )));
            }
            Ok(self.token.clone())
        })
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
