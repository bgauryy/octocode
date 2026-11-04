//! One request's context: its budget, its credential, and what it observed.
use octocode_io::budget::RequestBudget;
use std::{sync::Arc, time::Duration};
use url::Url;

use super::{ProviderEvidence, RequestSpec, evidence};
use crate::{ProviderError, ResolvedCredential};

/// One request's budget and the credential the host resolved for it. Every
/// GitHub call made with this context carries that credential (or none).
#[derive(Clone)]
pub struct RequestContext {
    pub budget: RequestBudget,
    credential: Option<ResolvedCredential>,
    /// What this request observed: what it saw, never a later one's.
    pub(super) journal: Arc<evidence::Journal>,
}
impl RequestContext {
    pub fn new(budget: RequestBudget, credential: Option<ResolvedCredential>) -> Self {
        Self {
            budget,
            credential,
            journal: Arc::default(),
        }
    }

    /// This request resumes an issued continuation that owns `evidence`:
    /// a request it recorded is answered from it, with nothing sent.
    #[must_use]
    pub fn resuming(mut self, evidence: Arc<ProviderEvidence>) -> Self {
        self.journal = Arc::new(evidence::Journal::resuming(evidence));
        self
    }

    /// Whether the continuation this request resumes owns the answer to `spec`.
    pub(crate) fn journal_answers(&self, spec: &RequestSpec) -> bool {
        self.journal.answers(spec)
    }

    /// Every answer and fact this request observed so far, replayed ones
    /// included: what a continuation it issues owns.
    pub fn evidence(&self) -> ProviderEvidence {
        self.journal.evidence()
    }

    /// The `full_name` a `/repos/{owner}/{repo}` read of this request
    /// returned: the repository's canonical name as this request saw it.
    pub fn read_full_name(&self, owner: &str, repo: &str) -> Option<String> {
        self.journal.note(&repository_note("fullName", owner, repo))
    }

    /// Remember the `full_name` a `/repos/{owner}/{repo}` read returned.
    pub(crate) fn record_full_name(&self, owner: &str, repo: &str, full_name: &str) {
        self.journal.set_note(
            repository_note("fullName", owner, repo),
            full_name.to_owned(),
        );
    }

    /// When the oldest answer this request reused without asking its origin
    /// was observed (milliseconds since the Unix epoch): what it may not show.
    pub fn reused_since(&self) -> Option<u64> {
        self.journal.note(REUSED_SINCE)?.parse().ok()
    }

    /// Remember that an answer observed at `observed_ms` was reused unasked.
    pub(super) fn record_reused(&self, observed_ms: u64) {
        let oldest = self
            .reused_since()
            .map_or(observed_ms, |known| known.min(observed_ms));
        self.journal
            .set_note(REUSED_SINCE.to_owned(), oldest.to_string());
    }

    /// The listing route this request found for `owner`'s repositories.
    pub(crate) fn owner_route(&self, owner: &str) -> Option<String> {
        self.journal
            .note(&format!("ownerRoute:{}", owner.to_ascii_lowercase()))
    }

    pub(crate) fn record_owner_route(&self, owner: &str, route: &str) {
        self.journal.set_note(
            format!("ownerRoute:{}", owner.to_ascii_lowercase()),
            route.to_owned(),
        );
    }

    /// GitHub redirected a request under `/repos/{owner}/{repo}` to the
    /// repository's id: the name is stale (renamed or transferred).
    pub fn followed_rename(&self, owner: &str, repo: &str) -> bool {
        self.journal
            .note(&repository_note("renamed", owner, repo))
            .is_some()
    }

    /// Remember the repository a followed `/repos/{owner}/{repo}/…` →
    /// `/repositories/<id>/…` redirect names.
    pub(super) fn record_rename(&self, from: &Url, to: &Url) {
        if !to.path().contains("/repositories/") {
            return;
        }
        let Some(segments) = from.path_segments() else {
            return;
        };
        let segments = segments.collect::<Vec<_>>();
        let Some(at) = segments.iter().position(|segment| *segment == "repos") else {
            return;
        };
        if let (Some(owner), Some(repo)) = (segments.get(at + 1), segments.get(at + 2)) {
            self.journal
                .set_note(repository_note("renamed", owner, repo), String::new());
        }
    }

    /// An anonymous request with a fresh `timeout` budget.
    pub fn with_timeout(timeout: Duration, max_body_bytes: usize) -> Self {
        Self::new(RequestBudget::with_timeout(timeout, max_body_bytes), None)
    }
    pub fn resolved_credential(&self) -> Option<&ResolvedCredential> {
        self.credential.as_ref()
    }
    /// The budget check as a provider error: cancellation first, then the deadline.
    pub fn check(&self) -> Result<(), ProviderError> {
        Ok(self.budget.check()?)
    }
}

/// The request fact naming when a reused answer was observed.
const REUSED_SINCE: &str = "reusedSince";

/// The name of a request fact about `owner/repo`, in GitHub's
/// case-insensitive spelling.
fn repository_note(fact: &str, owner: &str, repo: &str) -> String {
    format!("{fact}:{}", format!("{owner}/{repo}").to_ascii_lowercase())
}
