//! Direct translation of canonical typed history filters into provider constraints.
use super::GhSearchHistoryQuery as Q;
use crate::tools::gh_shared::ranges;
use crate::tools::variant_field::variant_field;
use octocode_contract_types::{
    GhSearchHistoryQueryPullRequestFilters, GitHubArchive, HistoryBranchSelection,
    HistoryCountSelection, HistoryDateSelection, HistoryPersonSelection,
};

/// Provider-ready typed constraints; all exclusions remain separate terms.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Filters {
    pub assignee: Option<String>,
    pub author: Option<String>,
    /// Commit committer (login or email); commits only.
    pub committer: Option<String>,
    pub commenter: Option<String>,
    pub mentions: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub closed: Option<String>,
    /// Commit-search committer date; exclusions stay in `excluded`.
    pub committed: Option<String>,
    pub comments: Option<String>,
    pub reactions: Option<String>,
    /// `linked:pr` / `linked:issue`: items linked to a pull request or issue.
    pub linked: Option<String>,
    /// Labels; all must match.
    pub label: Vec<String>,
    /// The text fields keywords search (`in:`).
    pub match_kinds: Vec<String>,
    pub state: Option<String>,
    pub archived: Option<bool>,
    pub draft: Option<bool>,
    /// `is:unmerged` / `-is:merged`: not merged, open or closed (pull requests).
    pub unmerged: Option<bool>,
    pub review_requested: Option<String>,
    pub reviewed_by: Option<String>,
    pub review: Option<String>,
    pub status: Option<String>,
    pub merged: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
    /// `-key:value` terms: rows matching them are excluded (search only).
    pub excluded: Vec<(String, String)>,
}

impl Filters {
    /// The person qualifiers besides `author`/`committer`, in query order.
    pub(super) fn people(&self) -> [(&'static str, Option<&str>); 5] {
        [
            ("assignee", self.assignee.as_deref()),
            ("mentions", self.mentions.as_deref()),
            ("commenter", self.commenter.as_deref()),
            ("reviewed-by", self.reviewed_by.as_deref()),
            ("review-requested", self.review_requested.as_deref()),
        ]
    }
}

fn selection<I: ToString, E: ToString>(
    key: &str,
    include: Option<&[I; 1]>,
    exclude: &[E],
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    denied.extend(
        exclude
            .iter()
            .map(|value| (key.to_owned(), value.to_string())),
    );
    include.map(|[value]| value.to_string())
}
fn person(
    key: &str,
    value: &Option<HistoryPersonSelection>,
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    value
        .as_ref()
        .and_then(|value| selection(key, value.include.as_ref(), &value.exclude, denied))
}
fn branch(
    key: &str,
    value: &Option<HistoryBranchSelection>,
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    value
        .as_ref()
        .and_then(|value| selection(key, value.include.as_ref(), &value.exclude, denied))
}
fn date(
    key: &str,
    value: &Option<HistoryDateSelection>,
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    let value = value.as_ref()?;
    denied.extend(
        value
            .exclude
            .iter()
            .filter_map(ranges::date)
            .map(|value| (key.to_owned(), value)),
    );
    value
        .include
        .as_ref()
        .and_then(|[value]| ranges::date(value))
}
fn count(
    key: &str,
    value: &Option<HistoryCountSelection>,
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    let value = value.as_ref()?;
    denied.extend(
        value
            .exclude
            .iter()
            .filter_map(ranges::numeric)
            .map(|value| (key.to_owned(), value)),
    );
    value
        .include
        .as_ref()
        .and_then(|[value]| ranges::numeric(value))
}
fn archive(value: Option<GitHubArchive>) -> Option<bool> {
    match value {
        Some(GitHubArchive::Exclude) => Some(false),
        Some(GitHubArchive::Only) => Some(true),
        _ => None,
    }
}
fn review(value: String) -> String {
    if value == "changesRequested" {
        "changes_requested".into()
    } else {
        value
    }
}
impl Filters {
    pub(super) fn of(query: &Q) -> Self {
        let mut filters = Self::default();
        filters.item_filters(query);
        match query {
            Q::Issue(query) if query.filters.is_some() => {
                filters.linked = filters.linked.map(|_| "pr".into());
                for (key, value) in &mut filters.excluded {
                    if key == "linked" {
                        *value = "pr".into();
                    }
                }
            }
            Q::PullRequest(query) => {
                if let Some(source) = &query.filters {
                    filters.pull_request_filters(source);
                }
            }
            _ => variant_field!(query, each [Q::CommitKeywords, Q::Commit3] query => {
                if let Some(source) = &query.filters {
                    filters.author = source
                        .author
                        .as_ref()
                        .map(|value| value.include[0].to_string());
                    filters.committer = source
                        .committer
                        .as_ref()
                        .map(|value| value.include[0].to_string());
                }
            }, else => {}),
        }
        if let Q::CommitKeywords(query) = query
            && let Some(source) = &query.filters
        {
            filters.committed = date(
                "committer-date",
                &source.committed_at,
                &mut filters.excluded,
            );
        }
        filters
    }

    /// The filters issues and pull requests share; commits have none of them.
    fn item_filters(&mut self, query: &Q) {
        variant_field!(query, each [Q::Issue, Q::PullRequest] query => {
            let Some(source) = &query.filters else { return };
            self.author = person("author", &source.author, &mut self.excluded);
            self.assignee = person("assignee", &source.assignee, &mut self.excluded);
            self.commenter = person("commenter", &source.commenter, &mut self.excluded);
            self.mentions = person("mentions", &source.mentions, &mut self.excluded);
            self.created = date("created", &source.created_at, &mut self.excluded);
            self.updated = date("updated", &source.updated_at, &mut self.excluded);
            self.closed = date("closed", &source.closed_at, &mut self.excluded);
            self.comments = count("comments", &source.comment_count, &mut self.excluded);
            self.reactions = count("reactions", &source.reaction_count, &mut self.excluded);
            self.state = source.state.map(|value| value.to_string());
            self.archived = archive(source.archive);
            self.match_kinds = source.match_in.iter().map(ToString::to_string).collect();
            if let Some(labels) = &source.labels {
                self.label = labels.include.iter().map(ToString::to_string).collect();
                self.excluded.extend(
                    labels
                        .exclude
                        .iter()
                        .map(|value| ("label".into(), value.to_string())),
                );
            }
            if let Some(linked) = &source.linked {
                self.linked = selection(
                    "linked",
                    linked.include.as_ref(),
                    &linked.exclude,
                    &mut self.excluded,
                );
            }
        }, else => {});
    }

    fn pull_request_filters(&mut self, source: &GhSearchHistoryQueryPullRequestFilters) {
        self.draft = source.is_draft;
        match source.is_merged {
            Some(true) => self.state = Some("merged".into()),
            Some(false) => self.unmerged = Some(true),
            None => {}
        }
        self.head = branch("head", &source.head_ref, &mut self.excluded);
        self.base = branch("base", &source.base_ref, &mut self.excluded);
        self.review_requested = person(
            "review-requested",
            &source.review_requested,
            &mut self.excluded,
        );
        self.reviewed_by = person("reviewed-by", &source.reviewed_by, &mut self.excluded);
        self.merged = date("merged", &source.merged_at, &mut self.excluded);
        if let Some(value) = &source.review {
            self.review = selection(
                "review",
                value.include.as_ref(),
                &value.exclude,
                &mut self.excluded,
            )
            .map(review);
            for (key, value) in &mut self.excluded {
                if key == "review" {
                    *value = review(value.clone());
                }
            }
        }
        if let Some(value) = &source.check_status {
            self.status = selection(
                "status",
                value.include.as_ref(),
                &value.exclude,
                &mut self.excluded,
            );
        }
    }
}
