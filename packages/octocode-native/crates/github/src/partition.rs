//! The one credential partition every cache, limiter, and rejection set
//! derives from: an origin plus a digest of the exact credential that reads
//! through it, so one token's answers and budgets never serve another caller.
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};

/// An origin (API URL, registry host, endpoint key) scoped by the exact
/// credential that reads it. The credential is kept only as a digest.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Partition {
    origin: String,
    /// `anon`, or the first 8 bytes of the credential digest in hex.
    tag: String,
    /// The full credential digest; `None` when anonymous.
    credential: Option<[u8; 32]>,
    /// Hex digest of the origin and the credential digest.
    identity: String,
}

impl Partition {
    /// `secret: None` or an empty secret is anonymous.
    pub fn new<S: ExposeSecret<str> + ?Sized>(
        origin: impl Into<String>,
        secret: Option<&S>,
    ) -> Self {
        let credential = secret
            .map(ExposeSecret::expose_secret)
            .filter(|secret| !secret.is_empty())
            .map(|secret| <[u8; 32]>::from(Sha256::digest(secret.as_bytes())));
        Self::from_parts(origin.into(), credential)
    }

    fn from_parts(origin: String, credential: Option<[u8; 32]>) -> Self {
        let tag = credential.map_or_else(|| "anon".to_owned(), |digest| hex::encode(&digest[..8]));
        let mut digest = Sha256::new();
        digest.update(b"octocode-partition-v1");
        digest.update((origin.len() as u64).to_le_bytes());
        digest.update(origin.as_bytes());
        match &credential {
            Some(credential) => {
                digest.update([1]);
                digest.update(credential);
            }
            None => digest.update([0]),
        }
        Self {
            identity: hex::encode(digest.finalize()),
            origin,
            tag,
            credential,
        }
    }

    /// A short, stable label for the credential (`anon` when anonymous): the
    /// per-credential part of persisted limiter file names.
    pub(crate) fn credential_tag(&self) -> &str {
        &self.tag
    }

    /// Opaque identity of origin and credential, for cache keys.
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

impl std::fmt::Debug for Partition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Partition")
            .field("origin", &self.origin)
            .field("credential", &self.tag)
            .finish()
    }
}

#[cfg(test)]
mod tests;
