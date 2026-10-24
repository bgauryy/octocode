//! The one GitHub search-term builder ghSearchRepo, ghSearchCode and
//! ghSearchHistory share: keywords, the `repo:`/`user:` scope and
//! qualifiers under one rule each, so a value can never start a new term
//! and a relative date window means the same date in every tool.
use octocode_github::{qualifier_value, quote_search_keyword};

/// The terms of one search query, in the order they are added.
#[derive(Debug, Default)]
pub(crate) struct SearchTerms(Vec<String>);

impl SearchTerms {
    /// Terms starting with `keywords`, each one term (quoted when it holds
    /// whitespace or search operators).
    pub(crate) fn new<'a>(keywords: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            keywords
                .into_iter()
                .map(quote_search_keyword)
                .filter(|term| !term.is_empty())
                .collect(),
        )
    }

    /// `repo:owner/repo` when both are named, else `user:owner`.
    pub(crate) fn scope(&mut self, owner: Option<&str>, repo: Option<&str>) {
        match (owner, repo) {
            (Some(owner), Some(repo)) => self.text("repo", Some(&format!("{owner}/{repo}"))),
            (owner, _) => self.text("user", owner),
        }
    }

    /// `key:value` as one term, quoted when the value holds whitespace,
    /// quotes, or parentheses.
    pub(crate) fn text(&mut self, key: &str, value: Option<&str>) {
        if let Some(value) = value {
            let value = qualifier_value(value);
            if !value.is_empty() {
                self.0.push(format!("{key}:{value}"));
            }
        }
    }

    /// A term the caller built from fixed text (`is:pr`, `archived:false`).
    pub(crate) fn push(&mut self, term: impl Into<String>) {
        self.0.push(term.into());
    }

    pub(crate) fn finish(self) -> String {
        self.0.join(" ").trim().into()
    }
}

/// A typed search query parsed from a JSON test fixture.
#[cfg(test)]
pub(crate) fn parse<Q: serde::de::DeserializeOwned>(value: serde_json::Value) -> Q {
    serde_json::from_value(value).expect("query fixture")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_values_cannot_open_a_second_scope() {
        let mut terms = SearchTerms::new(["parser repo:other/project"]);
        terms.scope(Some("owner"), Some("repo"));
        terms.text("label", Some("bug label:critical"));
        assert_eq!(
            terms.finish(),
            "\"parser repo:other/project\" repo:owner/repo label:\"bug label:critical\""
        );
    }
}
