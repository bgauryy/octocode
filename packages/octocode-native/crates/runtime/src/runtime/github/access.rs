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
    /// The provider evidence the continuation this call resumes owns, and
    /// the digest it is retained under.
    resumed: Option<(String, std::sync::Arc<ProviderEvidence>)>,
    /// What the call's rows observed, for the continuations it issues.
    observed: std::sync::Mutex<ProviderEvidence>,
}

impl ResponseAccess {
    /// A call resuming a continuation that owns `evidence`, retained under
    /// `digest`.
    pub(in crate::runtime) fn resuming(
        digest: String,
        evidence: std::sync::Arc<ProviderEvidence>,
    ) -> Self {
        Self {
            resumed: Some((digest, evidence)),
            ..Self::default()
        }
    }

    /// `request` resumes this call's evidence, if it has any.
    pub(super) fn resume(&self, request: RequestContext) -> RequestContext {
        match &self.resumed {
            Some((_, evidence)) => request.resuming(evidence.clone()),
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

    /// The digest the call's evidence is already retained under: its rows
    /// observed exactly what it resumed (a later page of one walk), so the
    /// continuations it issues own that retained evidence as it stands.
    pub(in crate::runtime) fn retained_digest(&self) -> Option<&str> {
        let (digest, resumed) = self.resumed.as_ref()?;
        let observed = self
            .observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (*observed == **resumed).then_some(digest.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_github::ProviderAnswer;
    use std::sync::Arc;

    fn evidence(body: &str) -> ProviderEvidence {
        ProviderEvidence {
            answers: vec![ProviderAnswer {
                request: "GET /file".into(),
                status: 200,
                headers: Vec::new(),
                body: body.into(),
                next: None,
                not_found: None,
            }],
            notes: Default::default(),
        }
    }

    /// Only a call that observed exactly what it resumed reuses that digest:
    /// any new answer is evidence of its own.
    #[test]
    fn only_an_unchanged_observation_keeps_the_resumed_digest() {
        assert_eq!(ResponseAccess::default().retained_digest(), None);
        let resumed = || ResponseAccess::resuming("digest".into(), Arc::new(evidence("body")));
        assert_eq!(resumed().retained_digest(), None, "nothing observed yet");
        let same = resumed();
        *same.observed.lock().unwrap() = evidence("body");
        assert_eq!(same.retained_digest(), Some("digest"));
        let changed = resumed();
        *changed.observed.lock().unwrap() = evidence("another body");
        assert_eq!(changed.retained_digest(), None);
    }
}
