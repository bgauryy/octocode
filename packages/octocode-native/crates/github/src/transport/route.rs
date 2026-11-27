//! What the saved-answer store keeps besides successes: the target of a
//! repository rename, and a not-found answer for a read that cannot change.
//!
//! A renamed repository answers its old name with a `301` to
//! `/repositories/{id}/…`. GitHub's redirect carries no lifetime and no
//! validator, so a kept route answers for [`RENAME_ROUTE_WINDOW`] and is
//! disclosed as reused, like any answer reused within a caller's window.
//! Every answer it routes keeps the rename disclosure.
//!
//! A `404` for a path inside a commit cannot change once the commit is known
//! to exist, so it is kept like the pinned success it replaces. The commit
//! counts as existing only when GitHub itself, in this request, returned it
//! (its commit, a tree or contents at that SHA) or answered a contents read
//! at it with a plain `Not Found` while its repository answered (GitHub says
//! "No commit found" for an unknown SHA there). A `404` for the object
//! itself (`git/trees/<sha>`, `git/blobs/<sha>`, `commits/<sha>`, a
//! comparison), one only a replayed answer vouches for, and any `404` for a
//! mutable ref are never kept: an object can be pushed later, and a
//! repository can be created or made visible.
//!
//! Proof comes from GitHub in the same request: a replayed answer, even one
//! inside the `max-age` GitHub stated, proves nothing now. A miss held for
//! want of it asks once, conditionally, for the repository's saved answer;
//! GitHub's `304` is free of the rate limit and is that proof. A kept miss
//! is saved under each name of its repository this request saw (the name
//! asked and a rename's id), as a success is.
//!
//! Assumption: a plain contents `404` names the path only because GitHub
//! answers a contents read at an unknown SHA with "No commit found for the
//! ref" (typed `RefNotFound`, never kept). That is checked live on
//! github.com only; a GitHub Enterprise Server that answered a plain
//! `Not Found` there would have such a miss kept. Unverified on GHES.
use std::time::Duration;
use url::Url;

use super::{AnswerBody, ProviderAnswer, SavedAnswer};
use crate::ProviderError;

/// How long a kept rename route answers without asking GitHub again.
/// GitHub's `301` states no lifetime and no validator, so this is a chosen
/// bound, like `SEARCH_REUSE_WINDOW`: each reuse is disclosed
/// ([`super::RequestContext::reused_since`]).
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "Disclosed cache freshness policy, not a request deadline or terminal evidence limit"
    )
)]
pub(crate) const RENAME_ROUTE_WINDOW: Duration = Duration::from_secs(300);

/// A path's not-found answer at a commit SHA, held until the commit is
/// proven to exist.
pub(crate) struct Missing {
    /// The URL that was read, naming its repository.
    pub(super) url: Url,
    /// The commit the path was read in.
    pub(super) commit: CommitRead,
    pub(super) scope: String,
    /// The store keys of the read: under the name asked, and under each
    /// other name of its repository (a rename's id).
    pub(super) requests: Vec<String>,
    pub(super) saved: SavedAnswer,
}

impl Missing {
    pub(super) fn new(
        url: &Url,
        commit: CommitRead,
        scope: String,
        requests: Vec<String>,
        error: ProviderError,
        now_ms: u64,
    ) -> Self {
        let answer = ProviderAnswer {
            request: requests.first().cloned().unwrap_or_default(),
            status: error.status.unwrap_or(404),
            headers: Vec::new(),
            body: AnswerBody::from(String::new()),
            next: None,
            not_found: Some(error),
        };
        Self {
            url: url.clone(),
            commit,
            scope,
            requests,
            saved: SavedAnswer {
                answer,
                observed_ms: now_ms,
                redirected: false,
            },
        }
    }
}

/// The store key of the route kept for the repository `prefix` names.
pub(super) fn route_key(prefix: &Url) -> String {
    format!("ROUTE {prefix}")
}

/// A kept route: the `/repositories/{id}` URL the old name led to.
pub(super) fn route_answer(prefix: &Url, target: &Url, now_ms: u64) -> SavedAnswer {
    SavedAnswer {
        answer: ProviderAnswer {
            request: route_key(prefix),
            status: 301,
            headers: vec![("location".to_owned(), target.to_string())],
            body: AnswerBody::from(String::new()),
            next: None,
            not_found: None,
        },
        observed_ms: now_ms,
        redirected: false,
    }
}

/// The target a kept route answer names.
pub(super) fn route_target(saved: &SavedAnswer) -> Option<Url> {
    let (_, location) = saved
        .answer
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("location"))?;
    Url::parse(location).ok()
}

/// The repository a `…/repos/{owner}/{repo}/…` URL names, as that URL
/// without what follows (owner and repository in GitHub's case-insensitive
/// spelling), with the path segments after it.
pub(super) fn repository_prefix(url: &Url) -> Option<(Url, Vec<String>)> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let at = segments.iter().position(|segment| *segment == "repos")?;
    let [owner, repo] = segments.get(at + 1..at + 3)? else {
        return None;
    };
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    let mut path = segments[..at + 1].join("/");
    path.push('/');
    path.push_str(&owner.to_ascii_lowercase());
    path.push('/');
    path.push_str(&repo.to_ascii_lowercase());
    let suffix = segments[at + 3..]
        .iter()
        .map(|segment| (*segment).to_owned())
        .collect();
    Some((bare(url, &path), suffix))
}

/// The `/repositories/{id}` URL a redirect target names, with the path
/// segments after it.
pub(super) fn repository_id_prefix(url: &Url) -> Option<(Url, Vec<String>)> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let at = segments
        .iter()
        .position(|segment| *segment == "repositories")?;
    let id = segments.get(at + 1)?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let path = segments[..at + 2].join("/");
    let suffix = segments[at + 2..]
        .iter()
        .map(|segment| (*segment).to_owned())
        .collect();
    Some((bare(url, &path), suffix))
}

/// A read at one commit SHA: which repository (its name or id URL) and
/// SHA, and whether it names a path inside the commit or the commit itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitRead {
    pub(super) repository: String,
    pub(super) sha: String,
    /// A path inside the commit, not the commit or its root tree.
    pub(super) path: bool,
    /// A `contents` read, where GitHub tells an unknown SHA apart.
    pub(super) contents: bool,
}

/// The commit a REST `url` reads at a full SHA: contents at `?ref=<sha>`,
/// a tree `<sha>` or `<sha>:<path>`, or `commits/<sha>`. A blob or a
/// comparison names no commit and path in it.
pub(super) fn commit_read(url: &Url) -> Option<CommitRead> {
    let (prefix, rest) = repository_prefix(url).or_else(|| repository_id_prefix(url))?;
    let read = |sha: &str, path: bool, contents: bool| {
        crate::is_full_sha(sha).then(|| CommitRead {
            repository: prefix.to_string(),
            sha: sha.to_ascii_lowercase(),
            path,
            contents,
        })
    };
    let rest = rest.iter().map(String::as_str).collect::<Vec<_>>();
    match rest.as_slice() {
        ["contents", path @ ..] => {
            let sha = url
                .query_pairs()
                .find_map(|(key, value)| (key == "ref").then(|| value.into_owned()))?;
            read(&sha, path.iter().any(|segment| !segment.is_empty()), true)
        }
        ["git", "trees", tree] => {
            let (sha, path) = tree
                .split_once(':')
                .or_else(|| tree.split_once("%3A"))
                .unwrap_or((tree, ""));
            read(sha, !path.is_empty(), false)
        }
        ["commits", sha] => read(sha, false, false),
        _ => None,
    }
}

fn bare(url: &Url, path: &str) -> Url {
    let mut bare = url.clone();
    bare.set_path(path);
    bare.set_query(None);
    bare.set_fragment(None);
    bare
}

/// The route a followed redirect proves: `from` and `to` name the same
/// resource, one under the old repository name and one under its id.
pub(crate) fn rename_route(from: &Url, to: &Url) -> Option<(Url, Url)> {
    let (old, old_suffix) = repository_prefix(from)?;
    let (new, new_suffix) = repository_id_prefix(to)?;
    (old_suffix == new_suffix && from.query() == to.query()).then_some((old, new))
}

/// `url` (under a repository's name or id) sent to the repository `target`
/// names instead: along a kept route, or back to the name it came from.
pub(super) fn routed(url: &Url, target: &Url) -> Option<Url> {
    let (_, suffix) = repository_prefix(url).or_else(|| repository_id_prefix(url))?;
    // The suffix is already encoded: joined as text, since pushing a segment
    // would encode its `%` a second time (`%2F` to `%252F`).
    let mut path = target.path().trim_end_matches('/').to_owned();
    for segment in &suffix {
        path.push('/');
        path.push_str(segment);
    }
    let mut routed = target.clone();
    routed.set_path(&path);
    routed.set_query(url.query());
    Some(routed)
}

#[cfg(test)]
mod tests;
