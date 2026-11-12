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
    /// Repositories GitHub answered for this request without a redirect.
    /// Kept apart from the journal: continuation evidence must not carry
    /// it, since a repository may be gone by the time a page resumes.
    resolved: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
    /// Commits GitHub returned for this request, as (repository URL, SHA):
    /// a path missing in one is the path's miss. Kept out of the journal
    /// like `resolved`.
    proven: Arc<std::sync::Mutex<std::collections::BTreeSet<(String, String)>>>,
    /// Not-found answers for paths at a commit SHA, waiting until the
    /// commit is proven: only then is the miss the path's, not the
    /// commit's or the repository's.
    missing: Arc<std::sync::Mutex<Vec<super::route::Missing>>>,
    /// Every rename route this request took, as (old repository URL, its
    /// `/repositories/{id}` URL, whether GitHub redirected it in this
    /// request): a repository that answers under its id answers for its
    /// old name.
    renamed: Arc<std::sync::Mutex<Vec<(Url, Url, bool)>>>,
    /// The batch's resolved refs, when the host resolved them.
    ref_memo: Option<Arc<crate::RefMemo>>,
    /// What this request waited on the client before its sends. Kept out of
    /// the journal: a resumed page did not wait the first page's waits.
    pub(crate) waits: Arc<crate::budget::ClientWaits>,
}
impl RequestContext {
    pub fn new(budget: RequestBudget, credential: Option<ResolvedCredential>) -> Self {
        Self {
            budget,
            credential,
            journal: Arc::default(),
            resolved: Arc::default(),
            proven: Arc::default(),
            missing: Arc::default(),
            renamed: Arc::default(),
            ref_memo: None,
            waits: Arc::default(),
        }
    }

    /// The client-side waits a limit made this request spend before its
    /// sends, summed by kind (a spent quota's reset, a secondary-limit
    /// cooldown, a `retry-after`): latency the provider did not cause. Routine
    /// spacing between starts is how every request is sent, not news to the
    /// caller: it is left out. Empty when no limit held it.
    pub fn client_waits(&self) -> Vec<octocode_contract_types::ClientWait> {
        let mut waits = self.waits.summed();
        waits.retain(|wait| wait.kind != octocode_contract_types::ClientWaitKind::Spacing);
        waits
    }

    #[must_use]
    pub fn with_ref_memo(mut self, memo: Arc<crate::RefMemo>) -> Self {
        self.ref_memo = Some(memo);
        self
    }

    pub(crate) fn ref_memo(&self) -> Option<&crate::RefMemo> {
        self.ref_memo.as_deref()
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

    /// GitHub answered a request under `/repos/{owner}/{repo}` of this
    /// request without a redirect: the repository resolves under that name,
    /// so a later 404 under it names something inside it.
    pub fn repository_resolves(&self, owner: &str, repo: &str) -> bool {
        self.resolved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&repository_note("resolves", owner, repo))
    }

    /// Remember that GitHub answered `url` (under `/repos/{owner}/{repo}`)
    /// without a redirect.
    pub(super) fn record_answered(&self, url: &Url) {
        if let Some((owner, repo)) = repository_of(url) {
            self.resolved
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(repository_note("resolves", owner, repo));
        }
    }

    /// Remember the repository a followed `/repos/{owner}/{repo}/…` →
    /// `/repositories/<id>/…` redirect names.
    pub(super) fn record_rename(&self, from: &Url, to: &Url) {
        if !to.path().contains("/repositories/") {
            return;
        }
        if let Some((owner, repo)) = repository_of(from) {
            self.journal
                .set_note(repository_note("renamed", owner, repo), String::new());
        }
    }

    /// A rename route GitHub just redirected along, from `old` to `new`.
    pub(super) fn record_route(&self, old: Url, new: Url) {
        lock(&self.renamed).push((old, new, true));
    }

    /// A kept rename route this request took from `old` to `new`.
    pub(super) fn record_route_taken(&self, old: Url, new: Url) {
        lock(&self.renamed).push((old, new, false));
    }

    /// The URLs `url` was sent to along the rename routes this request took
    /// or GitHub stated for its repository.
    pub(super) fn routed_urls(&self, url: &Url) -> Vec<Url> {
        let Some((prefix, _)) = super::route::repository_prefix(url) else {
            return Vec::new();
        };
        lock(&self.renamed)
            .iter()
            .filter(|(old, _, _)| *old == prefix)
            .filter_map(|(_, new, _)| super::route::routed(url, new))
            .collect()
    }

    /// Whether GitHub redirected the repository `old` names in this very
    /// request: a route it stated now, not one reused.
    pub(super) fn minted_route(&self, old: &Url) -> bool {
        lock(&self.renamed)
            .iter()
            .any(|(from, _, minted)| *minted && from == old)
    }

    /// The route taken for the repository `url` names led nowhere: its
    /// rename is no fact of this request.
    pub(super) fn forget_rename(&self, url: &Url) {
        if let Some((owner, repo)) = repository_of(url) {
            self.journal
                .remove_note(&repository_note("renamed", owner, repo));
        }
        if let Some((prefix, _)) = super::route::repository_prefix(url) {
            lock(&self.renamed).retain(|(old, _, _)| *old != prefix);
        }
    }

    /// GitHub returned `url`, a read at a commit SHA, in this request: the
    /// commit exists.
    pub(super) fn record_proven(&self, url: &Url) {
        if let Some(read) = super::route::commit_read(url) {
            lock(&self.proven).insert((read.repository, read.sha));
        }
    }

    /// The repository URLs that stand for `repository` in this request: it,
    /// and its other side of every rename route taken.
    pub(super) fn aliases(&self, repository: &str) -> Vec<String> {
        let mut aliases = vec![repository.to_owned()];
        for (old, new, _) in lock(&self.renamed).iter() {
            if old.as_str() == repository {
                aliases.push(new.to_string());
            } else if new.as_str() == repository {
                aliases.push(old.to_string());
            }
        }
        aliases
    }

    /// Whether the commit `read` names was returned in this request.
    fn commit_proven(&self, read: &super::route::CommitRead) -> bool {
        let aliases = self.aliases(&read.repository);
        let proven = lock(&self.proven);
        aliases
            .into_iter()
            .any(|repository| proven.contains(&(repository, read.sha.clone())))
    }

    /// Whether the repository `url` names answered in this request, under
    /// its name or, after a rename, under its id.
    fn answered_for(&self, url: &Url) -> bool {
        let resolves = |url: &Url| {
            repository_of(url).is_some_and(|(owner, repo)| self.repository_resolves(owner, repo))
        };
        resolves(url)
            || super::route::repository_prefix(url).is_some_and(|(prefix, _)| {
                lock(&self.renamed)
                    .iter()
                    .any(|(old, new, _)| *old == prefix && resolves(new))
            })
    }

    /// Takes over what `from` learned about repositories: which answered,
    /// which were renamed, and their canonical names. A request that reuses
    /// an answer `from` obtained (a batch's warmed ref) stands on the facts
    /// that answer proved.
    pub(crate) fn adopt_repository_facts(&self, from: &Self) {
        let resolved = lock(&from.resolved).clone();
        lock(&self.resolved).extend(resolved);
        let proven = lock(&from.proven).clone();
        lock(&self.proven).extend(proven);
        for (name, value) in from.journal.evidence().notes {
            if name.starts_with("renamed:") || name.starts_with("fullName:") {
                self.journal.set_note(name, value);
            }
        }
    }

    /// Holds a path's not-found answer at a commit SHA until the commit is
    /// proven.
    pub(super) fn hold_missing(&self, missing: super::route::Missing) {
        lock(&self.missing).push(missing);
    }

    /// Whether the read keyed `request` holds a contents miss that its
    /// repository's answer would prove.
    pub(super) fn awaits_repository(&self, request: &str) -> bool {
        lock(&self.missing).iter().any(|missing| {
            missing.commit.contents && missing.requests.iter().any(|key| key == request)
        })
    }

    /// The held not-found answers whose commit has since been proven: GitHub
    /// returned it, or answered a contents read at it with a plain
    /// `Not Found` (no "No commit found") while its repository answered.
    pub(super) fn take_proved_missing(&self) -> Vec<super::route::Missing> {
        let held = std::mem::take(&mut *lock(&self.missing));
        let (proved, waiting): (Vec<_>, Vec<_>) = held.into_iter().partition(|missing| {
            self.commit_proven(&missing.commit)
                || (missing.commit.contents && self.answered_for(&missing.url))
        });
        lock(&self.missing).extend(waiting);
        proved
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

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The `(owner, repo)` a REST URL under `…/repos/{owner}/{repo}` names.
/// Under `…/repositories/{id}` (where a rename leads) it is `("#", id)`:
/// `#` is no GitHub login, so an id never stands for a name.
pub(super) fn repository_of(url: &Url) -> Option<(&str, &str)> {
    let mut segments = url
        .path_segments()?
        .skip_while(|segment| !matches!(*segment, "repos" | "repositories"));
    if segments.next()? == "repositories" {
        let id = segments.next().filter(|id| !id.is_empty())?;
        return Some(("#", id));
    }
    let owner = segments.next().filter(|owner| !owner.is_empty())?;
    let repo = segments.next().filter(|repo| !repo.is_empty())?;
    Some((owner, repo))
}

/// The name of a request fact about `owner/repo`, in GitHub's
/// case-insensitive spelling.
fn repository_note(fact: &str, owner: &str, repo: &str) -> String {
    format!("{fact}:{}", format!("{owner}/{repo}").to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this request saw resolve is its own fact: a continuation that
    /// resumes it re-checks a repository that may have gone since.
    #[test]
    fn a_resolved_repository_does_not_travel_in_continuation_evidence() {
        let request = RequestContext::with_timeout(Duration::from_secs(5), 1024);
        let url = Url::parse("https://api.github.com/repos/a/b/pulls/1").expect("url");
        request.record_answered(&url);
        assert!(request.repository_resolves("A", "b"));
        let resumed = RequestContext::with_timeout(Duration::from_secs(5), 1024)
            .resuming(Arc::new(request.evidence()));
        assert!(!resumed.repository_resolves("a", "b"));
    }
}
