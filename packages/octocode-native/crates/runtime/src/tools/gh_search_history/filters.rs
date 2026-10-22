//! The `qualifiers` string parsed once into typed search filters.
use super::{GhSearchHistoryQuery, HistoryOperation};
use crate::tools::gh_search_history::gh_search_history_query;
use octocode_github::{ProviderError, ProviderErrorKind};

/// The filters a `qualifiers` string sets.
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

/// One filter a qualifier key sets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Field {
    Assignee,
    Author,
    Committer,
    Commenter,
    Mentions,
    Created,
    Updated,
    Closed,
    Comments,
    Reactions,
    Linked,
    Label,
    Match,
    State,
    Archived,
    Draft,
    Unmerged,
    ReviewRequested,
    ReviewedBy,
    Review,
    Status,
    Merged,
    Head,
    Base,
}

impl Field {
    /// Whether the qualifier's value is a GitHub login (or, for commits, an
    /// email): the ones GitHub search rejects when the user does not exist.
    fn is_person(self) -> bool {
        matches!(
            self,
            Self::Assignee
                | Self::Author
                | Self::Committer
                | Self::Commenter
                | Self::Mentions
                | Self::ReviewRequested
                | Self::ReviewedBy
        )
    }

    fn name(self) -> &'static str {
        match self {
            Self::Assignee => "assignee",
            Self::Author => "author",
            Self::Committer => "committer",
            Self::Commenter => "commenter",
            Self::Mentions => "mentions",
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Closed => "closed",
            Self::Comments => "comments",
            Self::Reactions => "reactions",
            Self::Linked => "linked",
            Self::Label => "label",
            Self::Match => "match",
            Self::State => "state",
            Self::Archived => "archived",
            Self::Draft => "draft",
            Self::Unmerged => "unmerged",
            Self::ReviewRequested => "review-requested",
            Self::ReviewedBy => "reviewed-by",
            Self::Review => "review",
            Self::Status => "status",
            Self::Merged => "merged",
            Self::Head => "head",
            Self::Base => "base",
        }
    }
}

/// The filter each `qualifiers` key sets. Which operations take a key is
/// the contract's (`x-qualifierKeys` per operation), never restated here.
const QUALIFIER_FIELDS: &[(&str, Field)] = &[
    ("assignee", Field::Assignee),
    ("author", Field::Author),
    ("committer", Field::Committer),
    ("commenter", Field::Commenter),
    ("mentions", Field::Mentions),
    ("created", Field::Created),
    ("updated", Field::Updated),
    ("closed", Field::Closed),
    ("comments", Field::Comments),
    ("reactions", Field::Reactions),
    ("linked", Field::Linked),
    ("label", Field::Label),
    ("in", Field::Match),
    ("is", Field::State),
    ("archived", Field::Archived),
    ("review-requested", Field::ReviewRequested),
    ("reviewed-by", Field::ReviewedBy),
    ("review", Field::Review),
    ("status", Field::Status),
    ("merged", Field::Merged),
    ("draft", Field::Draft),
    ("head", Field::Head),
    ("base", Field::Base),
];

/// `qualifiers` without its person terms (`author:x`, `-reviewed-by:y`, …):
/// what is left once the logins that failed to resolve are out of the way.
pub(super) fn without_person_terms(qualifiers: &str) -> String {
    qualifiers
        .split_whitespace()
        .filter(|term| {
            let key = term.trim_start_matches('-').split(':').next().unwrap_or("");
            !QUALIFIER_FIELDS
                .iter()
                .any(|(name, field)| *name == key && field.is_person())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn qualifier_error(message: String) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

/// The filter `key` sets. The typed query admits only the keys the
/// contract declares for its operation (`x-qualifierKeys`), so which
/// operation takes a key, unknown keys and scope keys are the contract's
/// rejections, never restated here.
fn qualifier_field(key: &str) -> Result<Field, ProviderError> {
    QUALIFIER_FIELDS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, field)| *field)
        .ok_or_else(|| qualifier_error(format!("qualifiers: unknown key {key}:.")))
}

/// A parsed qualifier value.
enum Setting {
    Text(String),
    Flag(bool),
    List(Vec<String>),
}

/// The filter and value one `key:value` term sets (`is:draft` sets draft).
fn setting(
    key: &str,
    field: Field,
    value: &str,
    negated: bool,
    pull_request: bool,
) -> Result<(Field, Setting), ProviderError> {
    Ok(match (field, value) {
        (Field::State, "draft") if pull_request => (Field::Draft, Setting::Flag(!negated)),
        // Not merged (open or closed): GitHub's `is:unmerged`, also spelled `-is:merged`.
        (Field::State, "unmerged") if pull_request && !negated => {
            (Field::Unmerged, Setting::Flag(true))
        }
        (Field::State, "merged") if pull_request && negated => {
            (Field::Unmerged, Setting::Flag(true))
        }
        (Field::State, "open" | "closed") if !negated => (field, Setting::Text(value.into())),
        (Field::State, "merged") if pull_request && !negated => {
            (field, Setting::Text(value.into()))
        }
        (Field::State, _) if negated => {
            return Err(qualifier_error(format!(
                "qualifiers: -is:{value}: set state instead."
            )));
        }
        (Field::State, "pr" | "issue" | "pull-request") => {
            return Err(qualifier_error(format!(
                "qualifiers: is:{value} is set by operation."
            )));
        }
        (Field::State, _) => {
            return Err(qualifier_error(format!(
                "qualifiers: is:{value} is not supported; use open, closed{}.",
                if pull_request {
                    ", merged, unmerged, or draft"
                } else {
                    ""
                }
            )));
        }
        (Field::Draft | Field::Archived, "true" | "false") if !negated => {
            (field, Setting::Flag(value == "true"))
        }
        (Field::Draft | Field::Archived, _) => {
            return Err(qualifier_error(format!(
                "qualifiers: {key}: takes true or false."
            )));
        }
        (Field::Match, _) if !negated => (
            field,
            Setting::List(value.split(',').map(str::to_owned).collect()),
        ),
        _ if negated => {
            return Err(qualifier_error(format!(
                "qualifiers: -{key}: (negation) is not supported."
            )));
        }
        _ => (field, Setting::Text(value.into())),
    })
}

impl Filters {
    /// Stores one filter; a filter set twice is rejected (labels add up).
    fn set(&mut self, key: &str, field: Field, value: Setting) -> Result<(), ProviderError> {
        if let (Field::Label, Setting::Text(label)) = (field, &value) {
            self.label.push(label.clone());
            return Ok(());
        }
        let repeated = || {
            qualifier_error(format!(
                "qualifiers: {key}: repeats the {} filter; set it once.",
                field.name()
            ))
        };
        let text = |slot: &mut Option<String>, value: Setting| match (slot.is_some(), value) {
            (false, Setting::Text(text)) => {
                *slot = Some(text);
                Ok(())
            }
            _ => Err(repeated()),
        };
        match field {
            Field::Assignee => text(&mut self.assignee, value),
            Field::Author => text(&mut self.author, value),
            Field::Committer => text(&mut self.committer, value),
            Field::Commenter => text(&mut self.commenter, value),
            Field::Mentions => text(&mut self.mentions, value),
            Field::Created => text(&mut self.created, value),
            Field::Updated => text(&mut self.updated, value),
            Field::Closed => text(&mut self.closed, value),
            Field::Comments => text(&mut self.comments, value),
            Field::Reactions => text(&mut self.reactions, value),
            Field::Linked => text(&mut self.linked, value),
            Field::State => text(&mut self.state, value),
            Field::ReviewRequested => text(&mut self.review_requested, value),
            Field::ReviewedBy => text(&mut self.reviewed_by, value),
            Field::Review => text(&mut self.review, value),
            Field::Status => text(&mut self.status, value),
            Field::Merged => text(&mut self.merged, value),
            Field::Head => text(&mut self.head, value),
            Field::Base => text(&mut self.base, value),
            Field::Archived | Field::Draft | Field::Unmerged => {
                let slot = match field {
                    Field::Draft => &mut self.draft,
                    Field::Unmerged => &mut self.unmerged,
                    _ => &mut self.archived,
                };
                match (slot.is_some(), value) {
                    (false, Setting::Flag(flag)) => {
                        *slot = Some(flag);
                        Ok(())
                    }
                    _ => Err(repeated()),
                }
            }
            Field::Match => match (self.match_kinds.is_empty(), value) {
                (true, Setting::List(kinds)) => {
                    self.match_kinds = kinds;
                    Ok(())
                }
                _ => Err(repeated()),
            },
            Field::Label => Err(repeated()),
        }
    }

    /// Parses a query's `qualifiers` string (its keys are the contract's):
    /// a filter set twice is rejected (an excluded value may repeat).
    pub(super) fn parse(query: &GhSearchHistoryQuery) -> Result<Self, ProviderError> {
        let mut filters = Self::default();
        let Some(text) = gh_search_history_query::qualifiers(query) else {
            return Ok(filters);
        };
        let operation = gh_search_history_query::operation(query);
        let pull_request = operation == HistoryOperation::PullRequest;
        // The term that asked for unmerged pull requests, as written.
        let mut unmerged = None;
        for term in crate::contracts::qualifier_terms(text) {
            let (negated, term) = match term.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, term.as_str()),
            };
            let Some((key, value)) = term
                .split_once(':')
                .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            else {
                return Err(qualifier_error(format!(
                    "qualifiers: \"{term}\" is not key:value; put free text in keywords."
                )));
            };
            let key = key.to_ascii_lowercase();
            let field = qualifier_field(&key)?;
            // A negated filter excludes what it names; the typed flags
            // (`is:`, `draft:`, `archived:`, `in:`) keep their own spellings.
            if negated
                && operation != HistoryOperation::Commit
                && !matches!(
                    field,
                    Field::State | Field::Draft | Field::Archived | Field::Match
                )
            {
                filters.excluded.push((key, value.to_owned()));
                continue;
            }
            let (field, value) = setting(&key, field, value, negated, pull_request)?;
            if field == Field::Unmerged {
                unmerged = Some(format!("{}{term}", if negated { "-" } else { "" }));
            }
            filters.set(&key, field, value)?;
        }
        if let Some(unmerged) = unmerged {
            filters.reject_merged_with(query, &unmerged)?;
        }
        Ok(filters)
    }

    /// `state:"merged"` (or `is:merged`) beside `unmerged` (the term that
    /// asked for unmerged pull requests) asks for none: GitHub answers it
    /// with rows of neither, so it is refused, naming both.
    fn reject_merged_with(
        &self,
        query: &GhSearchHistoryQuery,
        unmerged: &str,
    ) -> Result<(), ProviderError> {
        let wire = match query {
            GhSearchHistoryQuery::PullRequest { state, .. } => {
                state.as_ref().map(ToString::to_string)
            }
            _ => None,
        };
        let merged = match (wire.as_deref(), self.state.as_deref()) {
            (Some("merged"), _) => "state \"merged\"",
            (None, Some("merged")) => "is:merged",
            _ => return Ok(()),
        };
        Err(qualifier_error(format!(
            "qualifiers: {unmerged} contradicts {merged}: a pull request is merged or not; keep one."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::QUALIFIER_FIELDS;

    /// The `qualifiers` keys the contract declares for `operation`.
    fn contract_keys(operation: &str) -> Vec<&'static str> {
        crate::contracts::query_schema_value(
            crate::id::ToolId::GhSearchHistory,
            Some(operation),
            "qualifiers",
            "x-qualifierKeys",
        )
        .and_then(serde_json::Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect()
    }

    /// Every key the contract declares for an operation sets a native
    /// filter, and every native filter key is one the contract declares.
    #[test]
    fn every_contract_qualifier_key_sets_a_native_filter() {
        let mut declared = Vec::new();
        for operation in ["pullRequest", "issue", "commit"] {
            let keys = contract_keys(operation);
            assert!(!keys.is_empty(), "{operation}: no x-qualifierKeys");
            for key in keys {
                assert!(
                    QUALIFIER_FIELDS.iter().any(|(name, _)| *name == key),
                    "{operation}: contract key {key} has no native filter"
                );
                declared.push(key);
            }
        }
        for (name, _) in QUALIFIER_FIELDS {
            assert!(declared.contains(name), "{name}: no operation declares it");
        }
    }

    /// Which operation takes a key is the typed query's (the contract's)
    /// check: a key outside the operation's list never reaches the parser.
    #[test]
    fn a_key_outside_the_operations_list_is_rejected_by_the_typed_query() {
        for (operation, qualifiers) in [
            ("issue", "review:approved"),
            ("commit", "label:bug"),
            ("pullRequest", "committer:dev"),
            ("pullRequest", "repo:evil/x"),
        ] {
            let row = serde_json::json!({"operation":operation,"owner":"o","repo":"r",
                "qualifiers":qualifiers});
            assert!(
                serde_json::from_value::<crate::tools::gh_search_history::GhSearchHistoryQuery>(
                    row
                )
                .is_err(),
                "{operation} {qualifiers}"
            );
        }
    }
}
