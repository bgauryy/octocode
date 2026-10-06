//! Freshness of a tree page: each listed entry's last-commit date at the
//! listed commit, and that commit's own date, as `YYYY-MM-DD`.
use super::*;
use std::collections::HashMap;

/// Dates of one listing page.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct PageDates {
    /// The listed commit's date.
    pub(super) commit: Option<String>,
    /// Entry path (repository-relative) → its last-commit date.
    pub(super) entries: HashMap<String, String>,
    /// Entries left undated, stated as one row warning.
    #[serde(skip)]
    pub(super) warning: Option<String>,
}

/// A day is enough: entries of one page are compared by day, and the
/// listed commit's day bounds them all. Full timestamps would add 10 bytes
/// per entry for an ordering no listing reader asked for.
fn day(timestamp: &str) -> Option<String> {
    timestamp.get(..10).map(str::to_owned)
}

/// The page's dates: one cached record per (owner, repo, commit, page
/// paths), else one GraphQL request per 100 entries. A failure dates what
/// was answered and names the rest in [`PageDates::warning`].
pub(super) async fn page_dates<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    entries: &[TreeEntry],
    context: &RequestContext,
) -> PageDates {
    if entries.is_empty() {
        return PageDates::default();
    }
    let paths = entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let key = cache_key(owner, repo, commit_sha, &paths);
    let partition = provider.transport.cache_partition(context, None).await.ok();
    if let Some(partition) = &partition
        && let Some(cached) = provider.cache.get(partition, &key).await
        && let Ok(dates) = serde_json::from_slice::<PageDates>(&cached.bytes)
    {
        return dates;
    }
    let answer = provider
        .transport
        .path_commit_dates(owner, repo, commit_sha, &paths, context)
        .await;
    let mut dates = PageDates {
        commit: answer.commit_date.as_deref().and_then(day),
        entries: paths
            .iter()
            .zip(&answer.dates)
            .filter_map(|(path, date)| Some(((*path).to_owned(), day(date.as_deref()?)?)))
            .collect(),
        warning: None,
    };
    match answer.error {
        Some(error) => {
            let missing = paths.len() - dates.entries.len();
            dates.warning = Some(format!(
                "Last-commit dates (updated) are missing for {missing} {}: {}. Rerun the same query to retry.",
                if missing == 1 { "entry" } else { "entries" },
                error.message
            ));
        }
        None => {
            // A complete answer at a commit SHA never changes.
            if let Some(partition) = &partition
                && let Ok(bytes) = serde_json::to_vec(&dates)
            {
                provider
                    .cache
                    .put(
                        partition,
                        key,
                        crate::providers::github::CachedContent {
                            etag: None,
                            bytes,
                            resolved_ref: commit_sha.to_owned(),
                        },
                    )
                    .await;
            }
        }
    }
    dates
}

fn cache_key(owner: &str, repo: &str, commit_sha: &str, paths: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for value in [
        owner.to_ascii_lowercase().as_str(),
        repo.to_ascii_lowercase().as_str(),
        commit_sha,
    ]
    .into_iter()
    .chain(paths.iter().copied())
    {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("github-tree-dates:{}", hex::encode(digest.finalize()))
}

/// State the page's dates on its rows: each row's `updated` maps a listed
/// name to its date; the listed commit's date is `commitDate`. Row `dir`s
/// are repo-relative, like the dated paths.
pub(super) fn attach(value: &mut Value, dates: PageDates) {
    if let Some(commit) = dates.commit {
        value["commitDate"] = json!(commit);
    }
    if let Some(warning) = dates.warning {
        match value.get_mut("warnings").and_then(Value::as_array_mut) {
            Some(warnings) => warnings.push(json!(warning)),
            None => value["warnings"] = json!([warning]),
        }
    }
    if dates.entries.is_empty() {
        return;
    }
    let Some(rows) = value.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let dir = row["dir"].as_str().unwrap_or(".").to_owned();
        let mut updated = Map::new();
        for key in ["files", "folders"] {
            for name in row[key].as_array().into_iter().flatten() {
                let Some(name) = name.as_str() else { continue };
                let full = if dir == "." {
                    name.to_owned()
                } else {
                    format!("{dir}/{name}")
                };
                if let Some(date) = dates.entries.get(&full) {
                    updated.insert(name.to_owned(), json!(date));
                }
            }
        }
        if !updated.is_empty() {
            row["updated"] = Value::Object(updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_each_listed_name_to_its_repo_relative_date() {
        let mut value = json!({"entries": [
            {"dir": "src", "files": ["a.rs"], "folders": ["sub"]},
            {"dir": "src/sub", "files": ["b.rs"]}
        ]});
        let dates = PageDates {
            commit: Some("2026-02-03".into()),
            entries: [
                ("src/a.rs", "2025-01-01"),
                ("src/sub", "2025-02-02"),
                ("src/sub/b.rs", "2025-03-03"),
            ]
            .into_iter()
            .map(|(path, date)| (path.to_owned(), date.to_owned()))
            .collect(),
            warning: None,
        };
        attach(&mut value, dates);
        assert_eq!(value["commitDate"], "2026-02-03");
        assert_eq!(
            value["entries"][0]["updated"],
            json!({"a.rs": "2025-01-01", "sub": "2025-02-02"})
        );
        assert_eq!(
            value["entries"][1]["updated"],
            json!({"b.rs": "2025-03-03"})
        );
        assert!(value.get("warnings").is_none());
    }

    #[test]
    fn a_warning_joins_existing_row_warnings() {
        let mut value = json!({"entries": [], "warnings": ["first"]});
        let dates = PageDates {
            warning: Some("dates missing".into()),
            ..PageDates::default()
        };
        attach(&mut value, dates);
        assert_eq!(value["warnings"], json!(["first", "dates missing"]));
    }

    #[test]
    fn a_timestamp_shortens_to_its_day() {
        assert_eq!(day("2026-01-31T23:59:59Z").as_deref(), Some("2026-01-31"));
        assert_eq!(day("bad"), None);
    }
}
