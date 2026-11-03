//! The GitHub credential one dispatched batch selected and used, for saved-response access.
use crate::providers::github::CallSelection;
use octocode_github::{
    Partition, ProviderError, ProviderEvidence, RequestContext, ResolvedCredential,
};

/// Request-owned GitHub access of one call, never shared with another call:
/// the credential its rows send, resolved once, and the identities actually
/// used. Failed or mixed resolution cannot authorize saved pages.
#[derive(Default)]
pub(in crate::runtime) struct ResponseAccess {
    used: std::sync::Mutex<(std::collections::BTreeSet<String>, bool)>,
    selection: CallSelection,
    /// The provider evidence the continuation this call resumes owns.
    resumed: Option<std::sync::Arc<ProviderEvidence>>,
    /// What the call's rows observed, for the continuations it issues.
    observed: std::sync::Mutex<ProviderEvidence>,
}

impl ResponseAccess {
    /// A call resuming a continuation that owns `evidence`.
    pub(in crate::runtime) fn resuming(evidence: std::sync::Arc<ProviderEvidence>) -> Self {
        Self {
            resumed: Some(evidence),
            ..Self::default()
        }
    }

    /// `request` resumes this call's evidence, if it has any.
    pub(super) fn resume(&self, request: RequestContext) -> RequestContext {
        match &self.resumed {
            Some(evidence) => request.resuming(evidence.clone()),
            None => request,
        }
    }

    /// Keep what one row's request observed: a request two rows sent keeps
    /// its first answer.
    pub(super) fn collect(&self, request: &RequestContext) {
        let row = request.evidence();
        let mut observed = self
            .observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for answer in row.answers {
            if observed
                .answers
                .iter()
                .all(|kept| kept.request != answer.request)
            {
                observed.answers.push(answer);
            }
        }
        for (name, value) in row.notes {
            observed.notes.entry(name).or_insert(value);
        }
    }

    /// Everything the call's rows observed.
    pub(in crate::runtime) fn observed(&self) -> ProviderEvidence {
        self.observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(super) fn selection(&self) -> &CallSelection {
        &self.selection
    }

    pub(super) fn observe(&self, host: &str, request: &Result<RequestContext, ProviderError>) {
        let mut used = self
            .used
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
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if used.1 || used.0.len() > 1 {
            return Err(());
        }
        Ok(used.0.first().cloned())
    }
}
