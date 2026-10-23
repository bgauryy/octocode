//! Which credential identities one dispatched batch used, for saved-response access.
use octocode_github::{Partition, ProviderError, RequestContext, ResolvedCredential};

/// Credential identities actually used by one dispatched batch, never shared
/// with another call. Failed or mixed resolution cannot authorize saved pages.
#[derive(Default)]
pub(in crate::runtime) struct ResponseAccess(
    std::sync::Mutex<(std::collections::BTreeSet<String>, bool)>,
);

impl ResponseAccess {
    pub(super) fn observe(&self, host: &str, request: &Result<RequestContext, ProviderError>) {
        let mut used = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match request {
            Ok(request) => {
                used.0.insert(
                    Partition::new(
                        host,
                        request
                            .resolved_credential()
                            .map(ResolvedCredential::secret),
                    )
                    .identity()
                    .to_owned(),
                );
            }
            Err(_) => used.1 = true,
        }
    }

    pub(in crate::runtime) fn identity(&self) -> Result<Option<String>, ()> {
        let used = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if used.1 || used.0.len() > 1 {
            return Err(());
        }
        Ok(used.0.first().cloned())
    }
}
