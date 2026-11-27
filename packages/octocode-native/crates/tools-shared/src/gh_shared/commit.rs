//! Commit facts every GitHub history tool reads the same way: who made a
//! commit, which pull request a squash-merge headline names, the date a
//! commit row shows, and the one commit row.
use super::dates::utc_date;
use octocode_runtime_base::response::result::remove_null_fields;
use serde_json::{Value, json};

/// A commit person as one string: the GitHub login, else the git name
/// (emails stay out of default rows).
pub fn person(v: &Value, kind: &str) -> Value {
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
pub fn headline_pull_request(headline: &str) -> Option<u64> {
    let digits = headline.trim_end().strip_suffix(')')?.rsplit_once("(#")?.1;
    digits.parse().ok().filter(|number| *number > 0)
}

/// The pull request a commit message names, in the order of how directly it
/// says so: a squash headline's `(#123)` suffix, a merge commit's
/// `Merge pull request #123`, then a `PR-URL:` trailer that points at this
/// repository (`owner/repo`, any case). A trailer naming another repository
/// is not this repository's pull request.
pub fn commit_pull_request(message: &str, owner: &str, repo: &str) -> Option<u64> {
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
pub fn commit_date(commit: &Value) -> String {
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
/// comparison's or pull request's commits): `commitSha`, `committedAt`
/// ([`commit_date`]), `messageHeadline` and `author` (login, else git
/// name). A field the commit lacks is absent; any row's `commitSha` reads
/// the whole commit.
pub fn row(commit: &Value) -> Value {
    let mut row = json!({
        "commitSha": commit["sha"],
        "committedAt": commit_date(commit),
        "messageHeadline": headline(commit),
        "author": person(commit, "author"),
    });
    remove_null_fields(&mut row);
    row
}

/// A commit row ([`row`]) holding only `commitSha`, `committedAt`, `author`
/// and `messageHeadline`, as one line:
/// `"<commitSha> <committedAt> <author>: <messageHeadline>"`. A list states
/// the four keys of every such commit once, in its schema, instead of on each
/// row. A row holding any other field, lacking one of the four, or naming an
/// author a reader could not split off at the first `": "`, stays the object.
pub fn line(row: Value) -> Value {
    const FIELDS: [&str; 4] = ["commitSha", "committedAt", "author", "messageHeadline"];
    let text = |field: &str| row.get(field).and_then(Value::as_str);
    let only_the_four = row
        .as_object()
        .is_some_and(|object| object.len() == FIELDS.len())
        && FIELDS.iter().all(|field| text(field).is_some());
    // The SHA and date are single tokens; the author ends at the first ": ".
    let token =
        |field: &str| text(field).is_some_and(|value| !value.is_empty() && !value.contains(' '));
    let author = text("author").is_some_and(|value| !value.is_empty() && !value.contains(": "));
    if !(only_the_four && token("commitSha") && token("committedAt") && author) {
        return row;
    }
    let field = |name: &str| text(name).unwrap_or("");
    json!(format!(
        "{} {} {}: {}",
        field("commitSha"),
        field("committedAt"),
        field("author"),
        field("messageHeadline")
    ))
}

/// The message past its headline (the blank separator line dropped);
/// `None` for a one-line message.
pub fn message_body(commit: &Value) -> Option<&str> {
    let message = commit.pointer("/commit/message").and_then(Value::as_str)?;
    let (_, body) = message.split_once('\n')?;
    let body = body.trim_matches(['\n', '\r']).trim_end();
    (!body.is_empty()).then_some(body)
}

#[cfg(test)]
mod tests;
