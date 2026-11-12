//! The refs one batch resolved before its rows ran: a batch of file reads
//! resolves each repository ref once, and every row reads the answer
//! through its request context instead of asking again.
use super::{ProviderError, RequestContext};
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};
use tokio::sync::watch;

type RefKey = (String, String, String);

/// A ref's resolution and the spelling of the repository it was asked under.
type RefAnswer = ((String, String), Result<String, ProviderError>);

fn lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// What one batch resolved before its rows ran: the commits its refs name,
/// held by the batch and read by its rows' requests. A reader waits until
/// the refs are known, so a row's own work overlaps the resolution instead
/// of following it; a resolution that ends without answering leaves the
/// reader to ask for itself.
pub struct RefMemo {
    known: watch::Sender<bool>,
    refs: Mutex<HashMap<RefKey, RefAnswer>>,
    /// The request that resolved the refs: what it observed of each
    /// repository is what a row that reads its answer would have observed.
    observed: Mutex<Option<RequestContext>>,
}

impl Default for RefMemo {
    fn default() -> Self {
        Self::new()
    }
}

impl RefMemo {
    #[must_use]
    pub fn new() -> Self {
        Self {
            known: watch::channel(false).0,
            refs: Mutex::default(),
            observed: Mutex::default(),
        }
    }

    fn reference_key(owner: &str, repo: &str, reference: Option<&str>) -> RefKey {
        (
            lower(owner),
            lower(repo),
            reference.unwrap_or("HEAD").to_owned(),
        )
    }

    /// What resolving `reference` of `owner/repo` answered: a request that
    /// failed fails each row that reads it, with the same error.
    pub fn record_ref(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
        resolved: Result<String, ProviderError>,
    ) {
        self.refs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                Self::reference_key(owner, repo, reference),
                ((owner.to_owned(), repo.to_owned()), resolved),
            );
    }

    /// The request that resolved the batch's refs.
    pub fn observed_by(&self, request: &RequestContext) {
        *self.observed.lock().unwrap_or_else(PoisonError::into_inner) = Some(request.clone());
    }

    /// Adds the resolution's client-side waits so far to `row`, which waited
    /// on it: its admission was the row's wait too. The time the
    /// resolution's own sends took is provider latency, not recorded.
    pub(crate) fn adopt_waits(&self, row: &RequestContext) {
        if let Some(request) = self
            .observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            row.waits.adopt(&request.waits);
        }
    }

    /// Gives `row` what the refs' request observed of the repositories.
    pub(crate) fn adopt_observed(&self, row: &RequestContext) {
        if let Some(request) = self
            .observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            row.adopt_repository_facts(request);
        }
    }

    /// Every ref of the batch has been tried: readers may go on.
    pub fn refs_known(&self) {
        self.known
            .send_if_modified(|known| !std::mem::replace(known, true));
    }

    /// What the batch resolved `reference` to, once the refs are known;
    /// `None` when it did not try.
    pub async fn resolved_ref(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
    ) -> Option<Result<String, ProviderError>> {
        // A closed channel cannot happen while `self` lives; either way the
        // reader then reads what exists.
        let _ = self.known.subscribe().wait_for(|known| *known).await;
        self.refs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&Self::reference_key(owner, repo, reference))
            // An error names the repository as it was asked: only that
            // spelling reads it; another resolves for itself.
            .filter(|((asked_owner, asked_repo), resolved)| {
                resolved.is_ok() || (asked_owner == owner && asked_repo == repo)
            })
            .map(|(_, resolved)| resolved.clone())
    }
}

/// Settles a memo when dropped: a resolution that ends, fails or is aborted
/// never leaves a row waiting.
pub struct SettleOnDrop(pub std::sync::Arc<RefMemo>);

impl Drop for SettleOnDrop {
    fn drop(&mut self) {
        self.0.refs_known();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProviderErrorKind;

    /// An error a batch recorded for a ref names the repository as the row
    /// that reads it spells it: another spelling resolves for itself.
    #[tokio::test]
    async fn a_recorded_ref_error_serves_only_the_spelling_it_named() {
        let memo = RefMemo::new();
        let error = ProviderError::new(ProviderErrorKind::NotFound, "Repository A/B not found");
        memo.record_ref("A", "B", None, Err(error));
        memo.refs_known();
        assert!(memo.resolved_ref("A", "B", None).await.is_some());
        assert!(memo.resolved_ref("a", "b", None).await.is_none());
    }

    /// A reader waits until the refs are known, and a memo settled without
    /// an answer leaves the reader to resolve for itself.
    #[tokio::test]
    async fn a_reader_waits_for_the_refs_and_a_settled_memo_without_one_answers_none() {
        let memo = std::sync::Arc::new(RefMemo::new());
        let reader = tokio::spawn({
            let memo = memo.clone();
            async move { memo.resolved_ref("a", "b", Some("main")).await }
        });
        tokio::task::yield_now().await;
        assert!(!reader.is_finished(), "the reader waits for the refs");
        drop(SettleOnDrop(memo));
        assert!(reader.await.expect("reader").is_none());
    }
}
