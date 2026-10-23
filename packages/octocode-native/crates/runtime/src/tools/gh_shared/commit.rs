//! Commit facts every GitHub history tool reads the same way: who made a
//! commit, which pull request a squash-merge headline names, the date a
//! commit row shows, and the one commit row.
use super::dates::utc_date;
use crate::response::result::remove_null_fields;
use serde_json::{Value, json};

/// A commit person as one string: the GitHub login, else the git name
/// (emails stay out of default rows).
pub(crate) fn person(v: &Value, kind: &str) -> Value {
    v.pointer(&format!("/{kind}/login"))
        .and_then(Value::as_str)
        .filter(|login| !login.is_empty())
        .or_else(|| {
            v.pointer(&format!("/commit/{kind}/name"))
                .and_then(Value::as_str)
        })
        .map_or(Value::Null, |name| json!(name))
}

/// The pull request a squash-merge headline ends with: `subject (#123)`.
/// Only the trailing suffix counts, so a revert names the reverting PR.
pub(crate) fn headline_pull_request(headline: &str) -> Option<u64> {
    let digits = headline.trim_end().strip_suffix(')')?.rsplit_once("(#")?.1;
    digits.parse().ok().filter(|number| *number > 0)
}

/// The pull request a commit message names, in the order of how directly it
/// says so: a squash headline's `(#123)` suffix, a merge commit's
/// `Merge pull request #123`, then a `PR-URL:` trailer that points at this
/// repository (`owner/repo`, any case). A trailer naming another repository
/// is not this repository's pull request.
pub(crate) fn commit_pull_request(message: &str, owner: &str, repo: &str) -> Option<u64> {
    let headline = message.lines().next().unwrap_or("");
    headline_pull_request(headline)
        .or_else(|| merge_pull_request(headline))
        .or_else(|| trailer_pull_request(message, owner, repo))
}

fn merge_pull_request(headline: &str) -> Option<u64> {
    let rest = headline.strip_prefix("Merge pull request #")?;
    let digits = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    digits.parse().ok().filter(|number| *number > 0)
}

fn trailer_pull_request(message: &str, owner: &str, repo: &str) -> Option<u64> {
    message.lines().rev().find_map(|line| {
        let (key, url) = line.split_once(':')?;
        if !key.trim().eq_ignore_ascii_case("PR-URL") {
            return None;
        }
        let url = url.trim();
        let path = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))?;
        let mut parts = path.trim_end_matches('/').split('/');
        let (url_owner, url_repo, kind, number) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        (kind == "pull"
            && parts.next().is_none()
            && url_owner.eq_ignore_ascii_case(owner)
            && url_repo.eq_ignore_ascii_case(repo))
        .then(|| number.parse().ok())
        .flatten()
        .filter(|number| *number > 0)
    })
}

/// The date a commit row shows: the committer date, the one `since` and
/// `until` filter on and history orders by; the author date only when the
/// commit carries no committer date.
pub(crate) fn commit_date(commit: &Value) -> String {
    utc_date(
        commit
            .pointer("/commit/committer/date")
            .or_else(|| commit.pointer("/commit/author/date"))
            .and_then(Value::as_str),
    )
}

/// A commit's headline: the first line of its message.
pub(crate) fn headline(commit: &Value) -> &str {
    let message = commit
        .pointer("/commit/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    message.lines().next().unwrap_or("")
}

/// The one commit row of every history list (a search, a listing, a
/// comparison's or pull request's commits): `commitSha`, `date`
/// ([`commit_date`]), `messageHeadline` and `author` (login, else git
/// name). A field the commit lacks is absent; any row's `commitSha` reads
/// the whole commit.
pub(crate) fn row(commit: &Value) -> Value {
    let mut row = json!({
        "commitSha": commit["sha"],
        "date": commit_date(commit),
        "messageHeadline": headline(commit),
        "author": person(commit, "author"),
    });
    remove_null_fields(&mut row);
    row
}

/// The message past its headline (the blank separator line dropped);
/// `None` for a one-line message.
pub(crate) fn message_body(commit: &Value) -> Option<&str> {
    let message = commit.pointer("/commit/message").and_then(Value::as_str)?;
    let (_, body) = message.split_once('\n')?;
    let body = body.trim_matches(['\n', '\r']).trim_end();
    (!body.is_empty()).then_some(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_shows_the_date_its_range_filter_uses() {
        let rebased = json!({"commit": {
            "author": {"date": "2023-01-01T00:00:00Z"},
            "committer": {"date": "2024-06-01T00:00:00Z"}
        }});
        assert_eq!(commit_date(&rebased), "2024-06-01T00:00:00Z");
        let bare = json!({"commit": {"author": {"date": "2023-01-01T00:00:00Z"}}});
        assert_eq!(commit_date(&bare), "2023-01-01T00:00:00Z");
    }

    /// Every history list shows one commit row: headline, author, the
    /// committer date in UTC; a missing person is absent, not invented.
    #[test]
    fn every_history_list_shows_one_commit_row() {
        let commit = json!({"sha":"c1","author":{"login":"octo"},"commit":{
            "message":"fix: parser\n\nLong body.\nMore.\n",
            "author":{"name":"Ada","date":"2024-01-01T00:00:00Z"},
            "committer":{"date":"2024-02-01T02:00:00+02:00"}}});
        assert_eq!(
            row(&commit),
            json!({"commitSha":"c1","messageHeadline":"fix: parser","author":"octo",
                "date":"2024-02-01T00:00:00Z"})
        );
        assert_eq!(message_body(&commit), Some("Long body.\nMore."));
        let bare = json!({"sha":"c2","commit":{"message":"one line"}});
        assert_eq!(
            row(&bare),
            json!({"commitSha":"c2","messageHeadline":"one line","date":""})
        );
        assert_eq!(message_body(&bare), None);
    }

    #[test]
    fn squash_headline_names_its_pull_request() {
        assert_eq!(
            headline_pull_request("io: use `spawn_mandatory_blocking` (#8506)"),
            Some(8506)
        );
        assert_eq!(headline_pull_request("Merge branch main"), None);
        assert_eq!(headline_pull_request("fix (#abc)"), None);
        assert_eq!(headline_pull_request("revert (#12) partially"), None);
    }

    #[test]
    fn a_commit_message_names_its_pull_request_by_headline_merge_or_trailer() {
        let named = |message: &str| commit_pull_request(message, "nodejs", "node");
        assert_eq!(
            named("fix (#12)\n\nPR-URL: https://github.com/nodejs/node/pull/99"),
            Some(12)
        );
        assert_eq!(named("Merge pull request #34 from a/b\n\nbody"), Some(34));
        assert_eq!(
            named("url: fix\n\nPR-URL: https://github.com/nodejs/node/pull/65801\nReviewed-By: x"),
            Some(65801)
        );
        assert_eq!(
            named("url: fix\n\npr-url: https://github.com/NodeJS/Node/pull/7/"),
            Some(7)
        );
        // Another repository's pull request is not this one's.
        assert_eq!(
            named("x\n\nPR-URL: https://github.com/other/node/pull/5"),
            None
        );
        assert_eq!(
            named("x\n\nPR-URL: https://github.com/nodejs/node/issues/5"),
            None
        );
        assert_eq!(named("x\n\nRefs: #5"), None);
    }

    #[test]
    fn a_person_is_the_login_else_the_git_name() {
        let linked = json!({"author": {"login": "octo"}, "commit": {"author": {"name": "Ada"}}});
        assert_eq!(person(&linked, "author"), "octo");
        let unlinked = json!({"author": null, "commit": {"author": {"name": "Ada"}}});
        assert_eq!(person(&unlinked, "author"), "Ada");
        let empty_login = json!({"author": {"login": ""}, "commit": {"author": {"name": "Ada"}}});
        assert_eq!(person(&empty_login, "author"), "Ada");
        assert_eq!(person(&json!({}), "committer"), Value::Null);
    }
}
