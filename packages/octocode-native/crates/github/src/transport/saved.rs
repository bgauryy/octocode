//! Answers kept across requests and processes, and when one may answer again.
//!
//! The origin decides. A resource named by a commit, blob or tree SHA cannot
//! change, so its answer needs no question. A `Cache-Control: max-age` is the
//! origin's own statement of how long an answer stands. Past that, a validator
//! (`ETag`, `Last-Modified`) asks whether it still stands: GitHub answers 304
//! without charging its rate limit. Only an answer with none of these (GitHub
//! search) falls back to a short window the caller names. That reuse, `max-age`
//! reuse, and any answer served because the origin rate-limited the asking, are
//! each disclosed by the time the answer was observed.
use reqwest::header::{HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
}

/// What a saved answer can do for a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reuse {
    /// Answers as it is: the request names something that cannot change.
    Pinned,
    /// Answers as it is, within the time the origin (or the caller's window)
    /// allows; changes since it was observed are not shown.
    Fresh,
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
        if !self.redirected {
            if pinned {
                return Reuse::Pinned;
            }
            if stated_lifetime(headers).is_some_and(|lifetime| age < lifetime) {
                return Reuse::Fresh;
            }
        }
        if let Some((name, value)) = validator(headers) {
            return Reuse::Ask(name, value);
        }
        if !self.redirected && window.is_some_and(|window| age < window) {
            return Reuse::Fresh;
        }
        Reuse::Spent
    }

    /// The origin confirmed this answer at `now_ms` with `headers` (a 304):
    /// they replace what freshness reads (RFC 9111 section 3.2); the rest of
    /// the answer stays whole.
    pub fn confirm(&mut self, headers: &[(String, String)], now_ms: u64) {
        self.observed_ms = now_ms;
        for name in ["cache-control", "age", "etag", "last-modified"] {
            let Some(value) = header(headers, name) else {
                continue;
            };
            let value = value.to_owned();
            self.answer
                .headers
                .retain(|(saved_name, _)| !saved_name.eq_ignore_ascii_case(name));
            self.answer.headers.push((name.to_owned(), value));
        }
    }
}

/// Whether a success with these `headers` is worth keeping: something can
/// later vouch for it, and the origin did not forbid keeping it.
pub fn worth_keeping(headers: &[(String, String)], pinned: bool, window: Option<Duration>) -> bool {
    !has_directive(headers, "no-store")
        && (pinned
            || window.is_some()
            || validator(headers).is_some()
            || stated_lifetime(headers).is_some())
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn directives(headers: &[(String, String)]) -> impl Iterator<Item = &str> {
    header(headers, "cache-control")
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
}

fn has_directive(headers: &[(String, String)], name: &str) -> bool {
    directives(headers).any(|directive| directive.eq_ignore_ascii_case(name))
}

/// How long the origin says the answer stands from when it was observed:
/// `max-age` less the `Age` a shared cache reported.
fn stated_lifetime(headers: &[(String, String)]) -> Option<Duration> {
    if has_directive(headers, "no-cache") || has_directive(headers, "no-store") {
        return None;
    }
    let max_age = directives(headers).find_map(|directive| {
        let (name, value) = directive.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("max-age")
            .then(|| value.trim().parse::<u64>().ok())?
    })?;
    let age = header(headers, "age")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let lifetime = max_age.saturating_sub(age);
    (lifetime > 0).then(|| Duration::from_secs(lifetime))
}

fn validator(headers: &[(String, String)]) -> Option<(HeaderName, String)> {
    header(headers, "etag")
        .map(|etag| (IF_NONE_MATCH, etag.to_owned()))
        .or_else(|| {
            header(headers, "last-modified").map(|date| (IF_MODIFIED_SINCE, date.to_owned()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(headers: &[(&str, &str)]) -> SavedAnswer {
        SavedAnswer {
            answer: ProviderAnswer {
                request: "GET x".into(),
                status: 200,
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
                body: "{}".into(),
                next: None,
                not_found: None,
            },
            observed_ms: 1_000_000,
            redirected: false,
        }
    }

    /// The table: what each kind of answer does at an age. Ages are relative
    /// to each answer's own stated lifetime, never a copied constant.
    #[test]
    fn the_origin_decides_how_an_answer_may_be_reused() {
        let observed = 1_000_000;
        let ask = |etag: &str| Reuse::Ask(IF_NONE_MATCH, etag.to_owned());
        let private = [("Cache-Control", "private, max-age=60"), ("ETag", "\"e\"")];
        assert_eq!(
            saved(&private).reuse(false, None, observed + 59_000),
            Reuse::Fresh
        );
        assert_eq!(
            saved(&private).reuse(false, None, observed + 60_000),
            ask("\"e\"")
        );
        let no_cache = [("Cache-Control", "no-cache"), ("ETag", "\"e\"")];
        assert_eq!(saved(&no_cache).reuse(false, None, observed), ask("\"e\""));
        let aged = [("cache-control", "public, max-age=300"), ("age", "290")];
        assert_eq!(
            saved(&aged).reuse(false, None, observed + 9_000),
            Reuse::Fresh
        );
        assert_eq!(
            saved(&aged).reuse(false, None, observed + 10_000),
            Reuse::Spent
        );
        let dated = [("last-modified", "Mon, 02 Nov 2026 00:00:00 GMT")];
        assert_eq!(
            saved(&dated).reuse(false, None, observed),
            Reuse::Ask(IF_MODIFIED_SINCE, dated[0].1.to_owned())
        );
        let window = Duration::from_secs(30);
        let bare = [("cache-control", "no-cache")];
        assert_eq!(
            saved(&bare).reuse(false, Some(window), observed + 29_000),
            Reuse::Fresh
        );
        assert_eq!(
            saved(&bare).reuse(false, Some(window), observed + 30_000),
            Reuse::Spent
        );
        assert_eq!(saved(&bare).reuse(false, None, observed), Reuse::Spent);
        // A validator always wins over the window: it can tell the truth.
        let validated = [("etag", "\"e\"")];
        assert_eq!(
            saved(&validated).reuse(false, Some(window), observed),
            ask("\"e\"")
        );
        // Pinned needs nothing, however old.
        assert_eq!(
            saved(&[]).reuse(true, None, u64::MAX),
            Reuse::Pinned,
            "a pinned answer never ages"
        );
    }

    /// A redirected answer always asks, so the redirect itself is seen again.
    #[test]
    fn a_redirected_answer_is_never_reused_without_asking() {
        let mut answer = saved(&[("cache-control", "max-age=60"), ("etag", "\"e\"")]);
        answer.redirected = true;
        let ask = Reuse::Ask(IF_NONE_MATCH, "\"e\"".into());
        assert_eq!(answer.reuse(true, None, answer.observed_ms), ask);
        assert_eq!(answer.reuse(false, None, answer.observed_ms), ask);
        let mut bare = saved(&[]);
        bare.redirected = true;
        assert_eq!(
            bare.reuse(true, Some(Duration::from_secs(1)), bare.observed_ms),
            Reuse::Spent
        );
    }

    #[test]
    fn only_an_answer_something_can_vouch_for_is_kept() {
        let headers = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect::<Vec<_>>()
        };
        let window = Some(Duration::from_secs(1));
        assert!(!worth_keeping(&headers(&[]), false, None));
        assert!(worth_keeping(&headers(&[]), true, None));
        assert!(worth_keeping(&headers(&[]), false, window));
        assert!(worth_keeping(&headers(&[("etag", "\"e\"")]), false, None));
        assert!(worth_keeping(
            &headers(&[("cache-control", "max-age=1")]),
            false,
            None
        ));
        assert!(!worth_keeping(
            &headers(&[("cache-control", "no-store"), ("etag", "\"e\"")]),
            true,
            window
        ));
    }

    #[test]
    fn a_confirmation_restarts_the_answer_and_keeps_its_body() {
        let mut answer = saved(&[("etag", "\"old\""), ("x-kept", "1")]);
        let confirmed = [
            ("ETag".to_owned(), "\"new\"".to_owned()),
            ("Cache-Control".to_owned(), "max-age=9".to_owned()),
            ("x-ratelimit-remaining".to_owned(), "1".to_owned()),
        ];
        answer.confirm(&confirmed, 5);
        assert_eq!(answer.observed_ms, 5);
        assert_eq!(answer.answer.body, "{}");
        let value = |name: &str| header(&answer.answer.headers, name).map(str::to_owned);
        assert_eq!(value("etag").as_deref(), Some("\"new\""));
        assert_eq!(value("cache-control").as_deref(), Some("max-age=9"));
        assert_eq!(value("x-kept").as_deref(), Some("1"));
        assert_eq!(value("x-ratelimit-remaining"), None);
    }
}
