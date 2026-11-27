//! `data.repoCommits`: the commit an owner-wide page's rows of one
//! repository share, stated once.
use serde_json::{Map, Value, json};

const FIELD: &str = "repoCommits";
const COMMIT: &str = "commitSha";

/// States once, as `"<repo> <commitSha>"` in `data.repoCommits`, the commit
/// the listed rows of a repository were read at, where two or more share
/// it; those rows omit it. A repository's lone row keeps its own. Rows
/// stay flat in rank order and keep `repo`, so each finds its commit by its
/// repository on the page it is on. Only for a page of one owner's rows in
/// several repositories (rows of one repository leave both fields to
/// `data`), and only where the table is shorter than the commits it
/// replaces: a page never grows by it. Stating again restates from the
/// rows as they are now (a selection may have hidden some since).
pub(super) fn state(data: &mut Value) {
    restore(data);
    let Some(files) = data.get("files").and_then(Value::as_array) else {
        return;
    };
    fn text<'a>(row: &'a Value, field: &str) -> Option<&'a str> {
        row.get(field).and_then(Value::as_str)
    }
    let owners_differ = files
        .windows(2)
        .any(|pair| text(&pair[0], "owner") != text(&pair[1], "owner"));
    // Each repository in page order: the commit of its first row, its row
    // count, and whether every row names that commit.
    let mut repos: Vec<(&str, Option<&str>, usize, bool)> = Vec::new();
    for row in files {
        let Some(repo) = text(row, "repo") else {
            return;
        };
        let commit = text(row, COMMIT);
        match repos.iter_mut().find(|(name, ..)| *name == repo) {
            Some((_, first, rows, shared)) => {
                *rows += 1;
                *shared &= *first == commit;
            }
            None => repos.push((repo, commit, 1, commit.is_some())),
        }
    }
    if owners_differ || repos.len() < 2 {
        return;
    }
    let table: Vec<(String, String)> = repos
        .into_iter()
        .filter(|(_, _, rows, shared)| *shared && *rows > 1)
        .filter_map(|(repo, commit, rows, _)| {
            let commit = commit?;
            // What the rows' own members take against the one entry.
            let stated = rows * member_chars(COMMIT, commit);
            (stated > entry(repo, commit).len() + 3).then(|| (repo.to_owned(), commit.to_owned()))
        })
        .collect();
    let replaced: usize = table
        .iter()
        .map(|(repo, commit)| member_chars(COMMIT, commit) + entry(repo, commit).len() + 3)
        .sum();
    // The field's own key and brackets count against what it replaces.
    if table.is_empty() || replaced <= member_chars(FIELD, "") {
        return;
    }
    let Some(fields) = data.as_object_mut() else {
        return;
    };
    for row in fields
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        if in_table(&table, row) {
            row.shift_remove(COMMIT);
        }
    }
    let entries = table
        .iter()
        .map(|(repo, commit)| json!(entry(repo, commit)))
        .collect();
    // The table sits before the rows that read it.
    let at = fields.keys().position(|key| key == "files").unwrap_or(0);
    fields.shift_insert(at, FIELD.to_owned(), Value::Array(entries));
}

/// Gives each row back the commit an earlier [`state`] moved to the table.
fn restore(data: &mut Value) {
    let Some(fields) = data.as_object_mut() else {
        return;
    };
    let Some(Value::Array(entries)) = fields.shift_remove(FIELD) else {
        return;
    };
    let table: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|entry| entry.as_str()?.split_once(' '))
        .collect();
    for row in fields
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        let stated = row
            .get("repo")
            .and_then(Value::as_str)
            .and_then(|repo| table.iter().find(|(name, _)| *name == repo))
            .map(|(_, commit)| json!(commit));
        if let Some(commit) = stated
            && !row.contains_key(COMMIT)
        {
            row.insert(COMMIT.to_owned(), commit);
        }
    }
}

/// Whether `row`'s repository states its commit in `table`.
fn in_table(table: &[(String, String)], row: &Map<String, Value>) -> bool {
    row.get("repo")
        .and_then(Value::as_str)
        .is_some_and(|repo| table.iter().any(|(name, _)| name == repo))
}

/// A table entry: a repository name holds no space, so the first space
/// splits it from its commit.
fn entry(repo: &str, commit: &str) -> String {
    format!("{repo} {commit}")
}

/// The characters an object member `"key":"value",` takes.
fn member_chars(key: &str, value: &str) -> usize {
    key.len() + value.len() + 6
}

#[cfg(test)]
mod tests;
