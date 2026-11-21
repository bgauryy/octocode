//! An `owner` or `repo` value spelt as a GitHub location (`sharkdp/bat`,
//! `github.com/sharkdp`, `https://github.com/sharkdp/bat.git`) has one
//! reading: its owner and repository names. The row runs with those, and
//! the note says the form to send.
use super::ValidationIssue;
use serde_json::Value;

/// The GitHub host a location may start with, after its scheme.
const HOST: &str = "github.com";

/// Repair a row whose `owner` or `repo` failed its name pattern by holding
/// a GitHub location: the row's index and the note. `None` when the value
/// is no location, or names an owner or repository other than the row's
/// own (two readings).
pub(super) fn repair(input: &mut Value, issue: &ValidationIssue) -> Option<(usize, String)> {
    if issue.rule_id != "schema.pattern" {
        return None;
    }
    let [queries, index, field] = issue.path.as_slice() else {
        return None;
    };
    if queries != "queries" || !matches!(field.as_str(), "owner" | "repo") {
        return None;
    }
    let index = index.parse::<usize>().ok()?;
    let received = issue.received.as_ref()?;
    let (owner, repo) = location(received.as_str()?)?;
    let row = input.get_mut("queries")?.get_mut(index)?.as_object_mut()?;
    if row.get(field.as_str()) != Some(received) {
        return None;
    }
    let own = |name: &str| row.get(name).and_then(Value::as_str).map(str::to_owned);
    let agrees = |own: Option<String>, read: &str| {
        own.is_none_or(|own| own == *received || own.eq_ignore_ascii_case(read))
    };
    match field.as_str() {
        "owner" => {
            if !repo.is_none_or(|repo| agrees(own("repo"), repo)) {
                return None;
            }
        }
        _ => {
            // A bare name failed for another reason; only a location reads.
            repo?;
            if !agrees(own("owner"), owner) {
                return None;
            }
        }
    }
    let mut set = vec![("owner", owner)];
    set.extend(repo.map(|repo| ("repo", repo)));
    for (name, value) in &set {
        row.insert((*name).to_owned(), Value::from(*value));
    }
    let form = set
        .iter()
        .map(|(name, value)| format!("`{name}: \"{value}\"`"))
        .collect::<Vec<_>>()
        .join(", ");
    let sent = received.as_str().unwrap_or_default();
    Some((
        index,
        format!("read `{field}: \"{sent}\"` as {form}; send each name in its own field next time."),
    ))
}

/// The owner and, when named, repository of a GitHub location: an
/// optional `https://` or `http://` scheme and `www.`, then `github.com/`
/// and one or two names, or two names without a host; a trailing `.git` or
/// `/` is dropped. A path past the repository (`/tree/main`) is not read:
/// it names more than the two fields hold.
fn location(sent: &str) -> Option<(&str, Option<&str>)> {
    let text = sent.trim();
    let text = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .unwrap_or(text);
    let text = text.strip_prefix("www.").unwrap_or(text);
    let hosted_path = text
        .get(..HOST.len() + 1)
        .filter(|head| head.eq_ignore_ascii_case(&format!("{HOST}/")))
        .and_then(|_| text.get(HOST.len() + 1..));
    let hosted = hosted_path.is_some();
    let path = hosted_path.unwrap_or(text);
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut names = path.split('/');
    let owner = names.next().filter(|name| is_name(name))?;
    let repo = names.next();
    if names.next().is_some() || repo.is_some_and(|repo| !is_name(repo)) {
        return None;
    }
    // A bare name without a host is no location.
    if repo.is_none() && !hosted {
        return None;
    }
    Some((owner, repo))
}

/// Whether `name` is spelt as a GitHub user, organization or repository
/// name: letters, digits, `-`, `_` and `.`.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
