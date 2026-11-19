//! The refs one batch resolved before its rows ran: a batch of file reads
//! resolves each repository ref once, and every row reads the answer
//! through its request context instead of asking again.
use super::{ProviderError, RequestContext, ResponsePage};
use serde_json::Value;
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
    /// The batch also reads its rows' files ahead
    /// ([`crate::GitHubProvider::warm_files_at_commits`]): a row's file read
    /// waits for that before it asks for itself.
    warms_files: bool,
    /// The batch also reads its rows' pull requests in one GraphQL request:
    /// each row's own first-page read, by its document and variables (the
    /// request body), answered from it ([`Self::record_pull_request`]).
    warms_pull_requests: bool,
    pull_requests: Mutex<HashMap<String, ResponsePage>>,
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
            warms_files: false,
            warms_pull_requests: false,
            pull_requests: Mutex::default(),
        }
    }

    /// A memo whose batch also reads its rows' files ahead: a row's file
    /// read waits until the warm-up is done ([`Self::files_warmed`]).
    #[must_use]
    pub fn warming_files(mut self) -> Self {
        self.warms_files = true;
        self
    }

    /// A memo whose batch also reads its rows' pull requests ahead
    /// ([`Self::pull_request`]).
    #[must_use]
    pub fn warming_pull_requests(mut self) -> Self {
        self.warms_pull_requests = true;
        self
    }

    /// Waits until the batch's warm-up is done when it reads files ahead,
    /// so a row's read finds what it kept instead of racing it.
    pub(crate) async fn files_warmed(&self) {
        if self.warms_files {
            let _ = self.known.subscribe().wait_for(|known| *known).await;
        }
    }

    /// The answer the batch's aliased read gave a row's own pull-request
    /// read whose request body is `body`, once the warm-up is done; `None`
    /// when the batch did not read it, and the row asks for itself.
    pub(crate) async fn pull_request(&self, body: &str) -> Option<ResponsePage> {
        if !self.warms_pull_requests {
            return None;
        }
        let _ = self.known.subscribe().wait_for(|known| *known).await;
        self.pull_requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(body)
            .cloned()
    }

    /// Records `page` as the answer to the pull-request read whose request
    /// body (`{"query", "variables"}`) is `body`.
    pub fn record_pull_request(&self, body: &Value, page: ResponsePage) {
        self.pull_requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(body.to_string(), page);
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
mod tests;
