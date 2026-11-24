//! Direct translation of canonical typed history filters into provider constraints.
use super::GhSearchHistoryQuery as Q;
use octocode_contract_types::GhSearchHistoryQueryPullRequestFilters;
use octocode_tools_shared::gh_shared::ranges;
use octocode_tools_shared::variant_field::variant_field;

/// Provider-ready typed constraints.
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
    /// The whole `linked:` term: `linked:pr` (issues) or `linked:issue` (pull
    /// requests), negated (`-linked:…`) for `linked: false`.
    pub linked: Option<String>,
    /// Labels; all must match.
    pub label: Vec<String>,
    /// The text fields keywords search (`in:`).
    pub match_kinds: Vec<String>,
    pub state: Option<String>,
    /// `is:unmerged` / `-is:merged`: not merged, open or closed (pull requests).
    pub unmerged: Option<bool>,
    pub reviewed_by: Option<String>,
    pub merged: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
}

impl Filters {
    /// The person qualifiers besides `author`/`committer`, in query order.
    pub(super) fn people(&self) -> [(&'static str, Option<&str>); 4] {
        [
            ("assignee", self.assignee.as_deref()),
            ("mentions", self.mentions.as_deref()),
            ("commenter", self.commenter.as_deref()),
            ("reviewed-by", self.reviewed_by.as_deref()),
        ]
    }
}

/// A filter value as its provider text.
fn text(value: Option<&impl ToString>) -> Option<String> {
    value.map(ToString::to_string)
}
/// The search term of the boolean `linked` filter: `linked:<other>` keeps
/// items linked to the other kind, `-linked:<other>` (`false`) drops them.
fn linked_term(linked: bool, other: &str) -> String {
    format!("{}linked:{other}", if linked { "" } else { "-" })
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
            self.committed = source.committed_at.as_ref().and_then(ranges::date);
        }
    }

    /// The filters issues and pull requests share; commits have none of them.
    fn item_filters(&mut self, query: &Q) {
        // An issue links to a pull request (`pr`), a pull request to an issue.
        let other = if matches!(query, Q::Issue(_)) {
            "pr"
        } else {
            "issue"
        };
        variant_field!(query, each [Q::Issue, Q::PullRequest] query => {
            let Some(source) = &query.filters else { return };
            self.author = text(source.author.as_ref());
            self.assignee = text(source.assignee.as_ref());
            self.commenter = text(source.commenter.as_ref());
            self.mentions = text(source.mentions.as_ref());
            self.created = source.created_at.as_ref().and_then(ranges::date);
            self.updated = source.updated_at.as_ref().and_then(ranges::date);
            self.closed = source.closed_at.as_ref().and_then(ranges::date);
            self.comments = source.comments_count.as_ref().and_then(ranges::numeric);
            self.reactions = source.reaction_count.as_ref().and_then(ranges::numeric);
            self.state = source.state.map(|value| value.to_string());
            self.match_kinds = source.match_in.iter().map(ToString::to_string).collect();
            self.label = source.labels.iter().map(ToString::to_string).collect();
            self.linked = source.linked.map(|linked| linked_term(linked, other));
        }, else => {});
    }

    fn pull_request_filters(&mut self, source: &GhSearchHistoryQueryPullRequestFilters) {
        match source.is_merged {
            Some(true) => self.state = Some("merged".into()),
            Some(false) => self.unmerged = Some(true),
            None => {}
        }
        self.head = text(source.head_ref.as_ref());
        self.base = text(source.base_ref.as_ref());
        self.reviewed_by = text(source.reviewed_by.as_ref());
        self.merged = source.merged_at.as_ref().and_then(ranges::date);
    }
}
