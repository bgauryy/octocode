//! Answers kept across requests and processes, and when one may answer again.
//!
//! The origin decides. A resource named by a commit, blob or tree SHA cannot
//! change, so its answer needs no question. A `Cache-Control: max-age` is the
//! origin's own statement of how long an answer stands. Past that, a validator
//! (`ETag`, `Last-Modified`) asks whether it still stands: GitHub answers 304
//! without charging its rate limit. Only an answer with none of these (GitHub
//! search, a GraphQL read) falls back to a short window the caller names. That reuse, and any
//! answer served because the origin rate-limited the asking, are each disclosed
//! by the time the answer was observed; reuse within the origin's own `max-age`
//! is not, since the origin stands behind it.
use reqwest::header::{HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH};
use serde::{Deserialize, Serialize};
use std::time::{Duration, UNIX_EPOCH};

use super::ProviderAnswer;

/// An answer as it was kept.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SavedAnswer {
    pub answer: ProviderAnswer,
    /// When the origin last stood behind it (sent it or answered 304), in
    /// milliseconds since the Unix epoch.
    pub observed_ms: u64,
    /// It came through a redirect. A later request must see that redirect
    /// itself (a renamed repository is reported from it), so it always asks.
    pub redirected: bool,
}

/// Where answers are kept: a credential partition's `scope` (its
/// [`crate::Partition::identity`]) separates them, and `request` names one.
/// Everything may decline: a miss costs one more request.
pub trait SavedAnswers: Send + Sync {
    fn load(&self, scope: &str, request: &str) -> Option<SavedAnswer>;
    fn save(&self, scope: &str, request: &str, saved: SavedAnswer);
    fn forget(&self, scope: &str, request: &str);
    /// The origin refused `scope`'s credential (a 401): none of the answers
    /// kept for it may serve again, pinned ones included, since a credential
    /// that no longer opens the resource gives no right to its answers.
    /// A store with nothing to retire declines.
    fn revoked(&self, _scope: &str) {}
}

/// What a saved answer can do for a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reuse {
    /// Answers as it is: the request names something that cannot change.
    Pinned,
    /// Answers as it is, within the lifetime the origin itself stated
    /// (`max-age`): the origin vouches for it, so nothing is disclosed.
    Fresh,
    /// Answers as it is, within the caller's window for an answer the origin
    /// neither dated nor can validate; changes since it was observed are not
    /// shown, so the reuse is disclosed.
    Window,
    /// Must be confirmed by the origin with this header first.
    Ask(HeaderName, String),
    /// Cannot answer.
    Spent,
}

impl SavedAnswer {
    /// What this answer can do for a request at `now_ms`. `pinned`: the
    /// request names an immutable resource. `window`: how long an answer
    /// without a validator or a stated lifetime may be reused.
    pub fn reuse(&self, pinned: bool, window: Option<Duration>, now_ms: u64) -> Reuse {
        let headers = &self.answer.headers;
        let age = Duration::from_millis(now_ms.saturating_sub(self.observed_ms));
        if forbids_reuse(headers) {
            return Reuse::Spent;
        }
        let lifetime = stated_lifetime(headers, self.observed_ms);
        let must_validate = has_directive(headers, "no-cache")
            || (has_directive(headers, "must-revalidate")
                && lifetime.is_none_or(|lifetime| age >= lifetime));
        if !self.redirected && !must_validate {
            if pinned {
                return Reuse::Pinned;
            }
            if lifetime.is_some_and(|lifetime| age < lifetime) {
                return Reuse::Fresh;
            }
        }
        if let Some((name, value)) = validator(headers) {
            return Reuse::Ask(name, value);
        }
        if !self.redirected
            && !must_validate
            && !has_directive(headers, "max-age")
            && window.is_some_and(|window| age < window)
        {
            return Reuse::Window;
        }
        Reuse::Spent
    }

    /// The conditional header that asks the origin to confirm this answer,
    /// whatever lifetime it stated: what proves it still stands now.
    pub(super) fn revalidation(&self) -> Option<(HeaderName, String)> {
        if forbids_reuse(&self.answer.headers) {
            return None;
        }
        validator(&self.answer.headers)
    }

    /// A refused validation cannot override an origin's revalidation requirement.
    pub fn allows_stale(&self) -> bool {
        !forbids_reuse(&self.answer.headers)
            && !["no-cache", "must-revalidate"]
                .iter()
                .any(|name| has_directive(&self.answer.headers, name))
    }

    /// The origin confirmed this answer at `now_ms` with `headers` (a 304):
    /// they replace what freshness reads (RFC 9111 section 3.2); the rest of
    /// the answer stays whole.
    pub fn confirm(&mut self, headers: &[(String, String)], now_ms: u64) {
        self.observed_ms = now_ms;
        for name in [
            "cache-control",
            "date",
            "age",
            "etag",
            "last-modified",
            "vary",
        ] {
            if !headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(name))
            {
                continue;
            }
            self.answer
                .headers
                .retain(|(saved_name, _)| !saved_name.eq_ignore_ascii_case(name));
            self.answer.headers.extend(
                headers
                    .iter()
                    .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                    .cloned(),
            );
        }
    }
}

/// Whether a success with these `headers` is worth keeping: something can
/// later vouch for it, and the origin did not forbid keeping it.
pub fn worth_keeping(headers: &[(String, String)], pinned: bool, window: Option<Duration>) -> bool {
    !forbids_reuse(headers)
        && (!has_directive(headers, "no-cache") || validator(headers).is_some())
        && (pinned
            || window.is_some()
            || validator(headers).is_some()
            || stated_lifetime(headers, crate::budget::now_ms()).is_some())
}

/// A wildcard Vary never matches another request (RFC 9111 section 4.1).
/// Decline retention as well as reuse, including validators and stale fallback.
pub(super) fn forbids_reuse(headers: &[(String, String)]) -> bool {
    has_directive(headers, "no-store")
        || headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("vary"))
            .flat_map(|(_, value)| super::headers::list(value, ','))
            .any(|member| member == "*")
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn directives(headers: &[(String, String)]) -> impl Iterator<Item = &str> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("cache-control"))
        .flat_map(|(_, value)| super::headers::list(value, ','))
}

pub(super) fn has_directive(headers: &[(String, String)], name: &str) -> bool {
    directives(headers).any(|directive| {
        directive
            .split_once('=')
            .map_or(directive, |(key, _)| key)
            .trim()
            .eq_ignore_ascii_case(name)
    })
}

/// The `max-age` an answer states, in seconds, when it allows reuse at all.
pub(super) fn stated_max_age(headers: &[(String, String)]) -> Option<u64> {
    if has_directive(headers, "no-cache") || forbids_reuse(headers) {
        return None;
    }
    let mut lifetimes = directives(headers).filter_map(|directive| {
        let (name, value) = directive.split_once('=')?;
        name.trim().eq_ignore_ascii_case("max-age").then(|| {
            octocode_io::http::parse_delta_seconds(value.trim().trim_matches('"')).unwrap_or(0)
        })
    });
    let lifetime = lifetimes.next()?;
    // Conflicting freshness values are stale (RFC 9111 section 4.2.1).
    Some(if lifetimes.any(|other| other != lifetime) {
        0
    } else {
        lifetime
    })
}

/// Freshness remaining at receipt: max-age minus the larger of apparent age
/// and upstream Age (which the transport already increased by response delay).
fn stated_lifetime(headers: &[(String, String)], observed_ms: u64) -> Option<Duration> {
    let max_age = Duration::from_secs(stated_max_age(headers)?);
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::disallowed_methods,
            reason = "Converts the origin's HTTP Age seconds, not a work limit"
        )
    )]
    let age = header(headers, "age")
        .and_then(octocode_io::http::parse_age)
        .map(Duration::from_secs)
        .unwrap_or_default();
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::disallowed_methods,
            reason = "Converts the recorded receipt timestamp in Unix milliseconds, not a work limit"
        )
    )]
    let observed = UNIX_EPOCH.checked_add(Duration::from_millis(observed_ms))?;
    let apparent_age = header(headers, "date")
        .and_then(|value| octocode_io::http::parse_http_date(value, observed))
        .and_then(|date| observed.duration_since(date).ok())
        .unwrap_or_default();
    let lifetime = max_age.saturating_sub(age.max(apparent_age));
    (!lifetime.is_zero()).then_some(lifetime)
}

fn validator(headers: &[(String, String)]) -> Option<(HeaderName, String)> {
    header(headers, "etag")
        .map(|etag| (IF_NONE_MATCH, etag.to_owned()))
        .or_else(|| {
            header(headers, "last-modified").map(|date| (IF_MODIFIED_SINCE, date.to_owned()))
        })
}

#[cfg(test)]
mod tests;
