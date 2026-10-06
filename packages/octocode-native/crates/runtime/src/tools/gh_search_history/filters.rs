//! The `qualifiers` string parsed once into typed search filters.
use super::{GhSearchHistoryQuery, HistoryOperation};
use crate::providers::github::{ProviderError, ProviderErrorKind};

/// The filters a `qualifiers` string sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Filters {
    pub assignee: Option<String>,
    pub author: Option<String>,
    pub commenter: Option<String>,
    pub mentions: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub closed: Option<String>,
    pub comments: Option<String>,
    pub reactions: Option<String>,
    /// Labels; all must match.
    pub label: Vec<String>,
    /// The text fields keywords search (`in:`).
    pub match_kinds: Vec<String>,
    pub state: Option<String>,
    pub archived: Option<bool>,
    pub draft: Option<bool>,
    pub review_requested: Option<String>,
    pub reviewed_by: Option<String>,
    pub review: Option<String>,
    pub checks: Option<String>,
    pub merged_at: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
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
    Commenter,
    Mentions,
    Created,
    Updated,
    Closed,
    Comments,
    Reactions,
    Label,
    Match,
    State,
    Archived,
    Draft,
    ReviewRequested,
    ReviewedBy,
    Review,
    Checks,
    MergedAt,
    Head,
    Base,
}

impl Field {
    fn name(self) -> &'static str {
        match self {
            Self::Assignee => "assignee",
            Self::Author => "author",
            Self::Commenter => "commenter",
            Self::Mentions => "mentions",
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Closed => "closed",
            Self::Comments => "comments",
            Self::Reactions => "reactions",
            Self::Label => "label",
            Self::Match => "match",
            Self::State => "state",
            Self::Archived => "archived",
            Self::Draft => "draft",
            Self::ReviewRequested => "review-requested",
            Self::ReviewedBy => "reviewed-by",
            Self::Review => "review",
            Self::Checks => "checks",
            Self::MergedAt => "merged-at",
            Self::Head => "head",
            Self::Base => "base",
        }
    }
}

/// `qualifiers` keys, the filter each one sets, and whether it applies to
/// pull requests only.
const QUALIFIER_KEYS: &[(&str, Field, bool)] = &[
    ("assignee", Field::Assignee, false),
    ("author", Field::Author, false),
    ("commenter", Field::Commenter, false),
    ("mentions", Field::Mentions, false),
    ("created", Field::Created, false),
    ("updated", Field::Updated, false),
    ("closed", Field::Closed, false),
    ("comments", Field::Comments, false),
    ("reactions", Field::Reactions, false),
    ("label", Field::Label, false),
    ("in", Field::Match, false),
    ("is", Field::State, false),
    ("archived", Field::Archived, false),
    ("review-requested", Field::ReviewRequested, true),
    ("reviewed-by", Field::ReviewedBy, true),
    ("review", Field::Review, true),
    ("status", Field::Checks, true),
    ("checks", Field::Checks, true),
    ("merged", Field::MergedAt, true),
    ("merged-at", Field::MergedAt, true),
    ("draft", Field::Draft, true),
    ("head", Field::Head, true),
    ("base", Field::Base, true),
];
/// Scope comes from owner/repo/operation, never from free text.
const SCOPE_QUALIFIERS: &[&str] = &["repo", "org", "user", "owner", "type"];

fn qualifier_error(message: String) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

/// The qualifier key table entry for `key`, or the unknown-key error with a
/// suggestion.
fn qualifier_field(key: &str) -> Result<(Field, bool), ProviderError> {
    if let Some(&(_, field, pr_only)) = QUALIFIER_KEYS.iter().find(|(name, _, _)| *name == key) {
        return Ok((field, pr_only));
    }
    let suggestion = QUALIFIER_KEYS
        .iter()
        .map(|(name, _, _)| (crate::contracts::levenshtein(key, name), *name))
        .filter(|(distance, _)| *distance <= 2)
        .min();
    Err(qualifier_error(match suggestion {
        Some((_, name)) => format!("qualifiers: unknown key {key}:; did you mean {name}:?"),
        None => format!(
            "qualifiers: unknown key {key}:; allowed: {}.",
            QUALIFIER_KEYS
                .iter()
                .map(|(name, _, _)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }))
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
        (Field::State, "open" | "closed") if !negated => (field, Setting::Text(value.into())),
        (Field::State, "merged") if pull_request && !negated => {
            (field, Setting::Text(value.into()))
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
                    ", merged, or draft"
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
            Field::Commenter => text(&mut self.commenter, value),
            Field::Mentions => text(&mut self.mentions, value),
            Field::Created => text(&mut self.created, value),
            Field::Updated => text(&mut self.updated, value),
            Field::Closed => text(&mut self.closed, value),
            Field::Comments => text(&mut self.comments, value),
            Field::Reactions => text(&mut self.reactions, value),
            Field::State => text(&mut self.state, value),
            Field::ReviewRequested => text(&mut self.review_requested, value),
            Field::ReviewedBy => text(&mut self.reviewed_by, value),
            Field::Review => text(&mut self.review, value),
            Field::Checks => text(&mut self.checks, value),
            Field::MergedAt => text(&mut self.merged_at, value),
            Field::Head => text(&mut self.head, value),
            Field::Base => text(&mut self.base, value),
            Field::Archived | Field::Draft => {
                let slot = if field == Field::Draft {
                    &mut self.draft
                } else {
                    &mut self.archived
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

    /// Parses a query's `qualifiers` string: allowlisted keys only, scope
    /// qualifiers rejected, a filter set twice rejected, unknown keys get a
    /// suggestion.
    pub(super) fn parse(query: &GhSearchHistoryQuery) -> Result<Self, ProviderError> {
        let mut filters = Self::default();
        let Some(text) = query.qualifiers() else {
            return Ok(filters);
        };
        let pull_request = query.operation() == HistoryOperation::PullRequest;
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
            if SCOPE_QUALIFIERS.contains(&key.as_str()) {
                return Err(qualifier_error(format!(
                    "qualifiers: {key}: is not allowed; scope comes from owner/repo."
                )));
            }
            let (field, pr_only) = qualifier_field(&key)?;
            if pr_only && !pull_request {
                return Err(qualifier_error(format!(
                    "qualifiers: {key}: applies to pull requests only."
                )));
            }
            let (field, value) = setting(&key, field, value, negated, pull_request)?;
            filters.set(&key, field, value)?;
        }
        Ok(filters)
    }
}

#[cfg(test)]
mod tests {
    use crate::contracts::query_schema_value;
    use crate::tools::id::ToolId;
    use serde_json::Value;

    /// The contract's key list of one operation's `qualifiers`: the
    /// declared `x-qualifierKeys`, else the key alternation of its pattern.
    fn contract_keys(operation: &str) -> Vec<String> {
        let declared = query_schema_value(
            ToolId::GhSearchHistory,
            Some(operation),
            "qualifiers",
            "x-qualifierKeys",
        );
        if let Some(keys) = declared.and_then(Value::as_array) {
            return keys
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
        }
        let pattern = query_schema_value(
            ToolId::GhSearchHistory,
            Some(operation),
            "qualifiers",
            "pattern",
        )
        .and_then(Value::as_str)
        .expect("qualifiers pattern");
        let end = pattern.find("):").expect("key group");
        let start = pattern[..end].rfind("(?:").expect("key group start") + 3;
        pattern[start..end].split('|').map(str::to_owned).collect()
    }

    /// SH4 drift: native `QUALIFIER_KEYS` is the contract's key list, per
    /// operation (pull-request-only keys only on pull requests).
    #[test]
    fn native_qualifier_keys_match_the_contract() {
        for (operation, pull_request) in [("pullRequest", true), ("issue", false)] {
            let mut native = super::QUALIFIER_KEYS
                .iter()
                .filter(|(_, _, pr_only)| pull_request || !pr_only)
                .map(|(name, _, _)| (*name).to_owned())
                .collect::<Vec<_>>();
            let mut contract = contract_keys(operation);
            native.sort();
            contract.sort();
            assert_eq!(native, contract, "{operation}");
        }
    }
}
