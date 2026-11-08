//! Direct translation of canonical typed history filters into provider constraints.
use super::GhSearchHistoryQuery as Q;
use crate::tools::gh_shared::ranges;
use crate::tools::variant_field::variant_field;
use octocode_contract_types::{
    DateRange, GhSearchHistoryQueryPullRequestFilters, GitHubArchive, NumericRange,
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
    /// Commit-search committer date.
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
            (
                review_request_key(self.review_requested.as_deref().unwrap_or_default()),
                self.review_requested.as_deref(),
            ),
        ]
    }
}

/// GitHub names a team review request `team-review-requested:org/slug`; a
/// login keeps `review-requested:`. A slash marks the team.
fn review_request_key(value: &str) -> &'static str {
    if value.contains('/') {
        "team-review-requested"
    } else {
        "review-requested"
    }
}

/// A positive value and its exclusions: the exclusions join `denied` as
/// `-key:value` terms; a field and its `not<Field>` both reach GitHub.
fn text(
    key: &str,
    value: Option<&impl ToString>,
    not: &[impl ToString],
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    denied.extend(not.iter().map(|value| (key.to_owned(), value.to_string())));
    value.map(ToString::to_string)
}
fn date(
    key: &str,
    value: Option<&DateRange>,
    not: &[DateRange],
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    denied.extend(
        not.iter()
            .filter_map(ranges::date)
            .map(|value| (key.to_owned(), value)),
    );
    value.and_then(ranges::date)
}
fn count(
    key: &str,
    value: Option<&NumericRange>,
    not: &[NumericRange],
    denied: &mut Vec<(String, String)>,
) -> Option<String> {
    denied.extend(
        not.iter()
            .filter_map(ranges::numeric)
            .map(|value| (key.to_owned(), value)),
    );
    value.and_then(ranges::numeric)
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
/// The `linked:` qualifier value of a contract `linked` choice.
fn linked_term(value: impl ToString) -> String {
    match value.to_string().as_str() {
        "pullRequest" => "pr".into(),
        other => other.into(),
    }
}
impl Filters {
    pub(super) fn of(query: &Q) -> Self {
        let mut filters = Self::default();
        filters.item_filters(query);
        if let Q::PullRequest(query) = query
            && let Some(source) = &query.filters
        {
            filters.pull_request_filters(source);
        }
        filters.commit_filters(query);
        filters
    }

    fn commit_filters(&mut self, query: &Q) {
        variant_field!(query, each [Q::CommitKeywords, Q::Commit3] query => {
            if let Some(source) = &query.filters {
                self.author = source.author.as_ref().map(ToString::to_string);
                self.committer = source.committer.as_ref().map(ToString::to_string);
            }
        }, else => {});
        if let Q::CommitKeywords(query) = query
            && let Some(source) = &query.filters
        {
            self.committed = date(
                "committer-date",
                source.committed_at.as_ref(),
                &[],
                &mut self.excluded,
            );
        }
    }

    /// The filters issues and pull requests share; commits have none of them.
    fn item_filters(&mut self, query: &Q) {
        variant_field!(query, each [Q::Issue, Q::PullRequest] query => {
            let Some(source) = &query.filters else { return };
            let denied = &mut self.excluded;
            self.author = text("author", source.author.as_ref(), &source.not_author, denied);
            self.assignee = text("assignee", source.assignee.as_ref(), &source.not_assignee, denied);
            self.commenter = text("commenter", source.commenter.as_ref(), &source.not_commenter, denied);
            self.mentions = text("mentions", source.mentions.as_ref(), &source.not_mentions, denied);
            self.created = date("created", source.created_at.as_ref(), &source.not_created_at, denied);
            self.updated = date("updated", source.updated_at.as_ref(), &source.not_updated_at, denied);
            self.closed = date("closed", source.closed_at.as_ref(), &source.not_closed_at, denied);
            self.comments = count("comments", source.comment_count.as_ref(), &source.not_comment_count, denied);
            self.reactions = count("reactions", source.reaction_count.as_ref(), &source.not_reaction_count, denied);
            self.state = source.state.map(|value| value.to_string());
            self.archived = archive(source.archive);
            self.match_kinds = source.match_in.iter().map(ToString::to_string).collect();
            self.label = source.labels.iter().map(ToString::to_string).collect();
            denied.extend(
                source.not_labels.iter().map(|value| ("label".into(), value.to_string())),
            );
            self.linked = source.linked.as_ref().map(linked_term);
            denied.extend(source.not_linked.as_ref().map(|value| ("linked".into(), linked_term(value))));
        }, else => {});
    }

    fn pull_request_filters(&mut self, source: &GhSearchHistoryQueryPullRequestFilters) {
        self.draft = source.is_draft;
        match source.is_merged {
            Some(true) => self.state = Some("merged".into()),
            Some(false) => self.unmerged = Some(true),
            None => {}
        }
        let denied = &mut self.excluded;
        self.head = text(
            "head",
            source.head_ref.as_ref(),
            &source.not_head_ref,
            denied,
        );
        self.base = text(
            "base",
            source.base_ref.as_ref(),
            &source.not_base_ref,
            denied,
        );
        self.review_requested = source.review_requested.as_ref().map(ToString::to_string);
        denied.extend(source.not_review_requested.iter().map(|value| {
            let value = value.to_string();
            (review_request_key(&value).to_owned(), value)
        }));
        self.reviewed_by = text(
            "reviewed-by",
            source.reviewed_by.as_ref(),
            &source.not_reviewed_by,
            denied,
        );
        self.merged = date(
            "merged",
            source.merged_at.as_ref(),
            &source.not_merged_at,
            denied,
        );
        self.review =
            text("review", source.review.as_ref(), &source.not_review, denied).map(review);
        self.status = text(
            "status",
            source.check_status.as_ref(),
            &source.not_check_status,
            denied,
        );
        for (key, value) in denied.iter_mut() {
            if key == "review" {
                *value = review(std::mem::take(value));
            }
        }
    }
}
