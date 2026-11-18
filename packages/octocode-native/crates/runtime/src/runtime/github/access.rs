//! The GitHub credential one dispatched batch selected and used, for saved-response access.
use octocode_github::{
    Partition, ProviderError, ProviderEvidence, RequestContext, ResolvedCredential,
};
use octocode_runtime_base::providers::github::CallSelection;

/// Request-owned GitHub access of one call, never shared with another call:
/// the credential its rows send, resolved once, and the identities actually
/// used. Failed or mixed resolution cannot authorize saved pages.
pub(in crate::runtime) struct ResponseAccess {
    /// When the call began, in milliseconds since the Unix epoch.
    started_ms: u64,
    used: std::sync::Mutex<(std::collections::BTreeSet<String>, bool)>,
    selection: CallSelection,
    /// The provider evidence the continuation this call resumes owns, and
    /// the digest it is retained under.
    resumed: Option<(String, std::sync::Arc<ProviderEvidence>)>,
    /// What the call's rows observed, for the continuations it issues.
    observed: std::sync::Mutex<ProviderEvidence>,
}

impl Default for ResponseAccess {
    /// A call that begins now.
    fn default() -> Self {
        Self {
            started_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| {
                    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
                }),
            used: std::sync::Mutex::default(),
            selection: CallSelection::default(),
            resumed: None,
            observed: std::sync::Mutex::default(),
        }
    }
}

impl ResponseAccess {
    /// Whether an answer observed at `observed_ms` predates this call (or
    /// came in the millisecond it began): an
    /// older observation, which a row reusing it discloses (`observedAt`).
    /// An answer one of this call's own rows observed is as fresh as the
    /// call.
    pub(super) fn predates(&self, observed_ms: u64) -> bool {
        observed_ms <= self.started_ms
    }

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
mod tests;
