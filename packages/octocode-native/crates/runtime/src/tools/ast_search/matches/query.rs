use crate::tools::variant_field::non_empty;
use octocode_contract_types::AstSearchQuery;
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_contract_types::AstSearchQueryMatchView;
use octocode_engine::num::saturating_u32;
use serde_json::Value;
use serde_json::json;
use std::num::NonZeroU64;

/// How a continuation of a match page moves its query: to the next file
/// page (its matches restart), to the next match page of the shown files, or
/// past the files a scan window evaluated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Move {
    FilePage(u64),
    MatchPage(u64),
    ScanWindow(u64),
}

/// A `match` query in its content or summary view.
#[derive(Clone, Copy, Debug)]
pub enum MatchQuery<'a> {
    Content(&'a AstSearchQueryMatch0),
    Summary(&'a AstSearchQueryMatchView),
}

/// Binds `$field` from whichever form `$query` is.
macro_rules! either_form {
    ($query:expr, $field:ident => $value:expr) => {
        crate::tools::variant_field::variant_field!($query, newtype [
            MatchQuery::Content => AstSearchQueryMatch0,
            MatchQuery::Summary => AstSearchQueryMatchView,
        ] $field => $value)
    };
}

/// The wire rule as the JSON object the engine decodes. The typed rule holds
/// only serializable data, so it always converts.
fn to_rule_value(rule: &octocode_contract_types::AstSearchRule) -> Value {
    serde_json::to_value(rule).unwrap_or(Value::Null)
}

/// Form-independent views in the engine's units.
impl<'a> MatchQuery<'a> {
    pub fn path(self) -> String {
        either_form!(self, path => path.to_string())
    }
    pub fn scope(self) -> Option<&'a octocode_contract_types::AstMatchScope> {
        either_form!(self, scope => scope.as_ref())
    }
    pub fn rule(self) -> Option<Value> {
        either_form!(self, rule => Some(to_rule_value(rule)))
    }
    pub fn pattern_text(self) -> Option<String> {
        self.rule()?.get("pattern")?.as_str().map(str::to_owned)
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn default_excludes(self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        either_form!(self, default_excludes => default_excludes.defaults())
    }
    pub fn test_code(self) -> Option<octocode_contract_types::TestCode> {
        either_form!(self, test_code => *test_code)
    }
    pub fn include_hidden(self) -> Option<bool> {
        either_form!(self, include_hidden => *include_hidden)
    }
    pub fn include_ignored(self) -> Option<bool> {
        either_form!(self, include_ignored => *include_ignored)
    }
    pub fn reverse(self) -> Option<bool> {
        either_form!(self, sort => Some(sort.order == octocode_contract_types::SortOrder::Desc))
    }
    pub fn capture_text(self) -> Option<bool> {
        match self {
            Self::Content(query) => query.include_captures,
            Self::Summary(_) => None,
        }
    }
    /// Per-match byte ranges, origins and digests (`includeProvenance`).
    pub fn provenance(self) -> bool {
        match self {
            Self::Content(query) => query.include_provenance.unwrap_or(false),
            Self::Summary(_) => false,
        }
    }
    /// Levels below `path` (1 = its children), as the wire counts them.
    pub fn max_depth(self) -> Option<u32> {
        either_form!(self, max_depth => max_depth.map(|depth| saturating_u32(depth.get())))
    }
    /// Leading candidates (walk order) an earlier window evaluated.
    pub fn scan_offset(self) -> u32 {
        either_form!(self, scan_offset => scan_offset.map_or(0, |offset| saturating_u32(offset.get())))
    }
    pub fn language(self) -> Option<String> {
        either_form!(self, parser => parser.as_ref().map(ToString::to_string))
    }
    pub fn language_globs(self) -> Option<std::collections::BTreeMap<String, Vec<String>>> {
        either_form!(self, parser_globs => parser_globs.as_ref().map(crate::tools::ast_rule::parser_globs))
    }
    pub fn sort(self) -> Option<&'static str> {
        use octocode_contract_types::AstSearchSortBy;
        either_form!(self, sort => Some(match sort.by {
            AstSearchSortBy::MatchCount => "matchCount",
            AstSearchSortBy::Path => "path",
        }))
    }
    pub fn result_view(self) -> Option<String> {
        either_form!(self, view => Some(view.to_string()))
    }
    pub fn page(self) -> u32 {
        either_form!(self, page => saturating_u32(page.get()))
    }
    /// Rows per page (`limit`): matches in view content, files in the
    /// file views; `None` cuts pages by the budget alone.
    pub fn limit(self) -> Option<usize> {
        either_form!(self, limit => limit.map(|limit| octocode_engine::num::saturating_usize(limit.get())))
    }
    pub fn match_page(self) -> u32 {
        either_form!(self, match_page => saturating_u32(match_page.get()))
    }
    pub fn snapshot(self) -> Option<String> {
        either_form!(self, snapshot => snapshot.as_ref().map(ToString::to_string))
    }
    /// The query as a continuation copies it.
    pub(super) fn query(self) -> AstSearchQuery {
        match self {
            Self::Content(query) => AstSearchQuery::Match0(query.clone()),
            Self::Summary(query) => AstSearchQuery::MatchView(query.clone()),
        }
    }

    /// This query moved to `to`, bound to the `snapshot` its pages share;
    /// `None` when `snapshot` is no valid token.
    pub(super) fn moved(self, to: Move, snapshot: &str) -> Option<AstSearchQuery> {
        macro_rules! moved {
            ($query:expr, $variant:path) => {{
                let mut query = $query.clone();
                match to {
                    Move::FilePage(page) => {
                        query.page = NonZeroU64::new(page)?;
                        query.match_page = NonZeroU64::MIN;
                        query.snapshot = Some(snapshot.parse().ok()?);
                    }
                    Move::MatchPage(page) => {
                        query.match_page = NonZeroU64::new(page)?;
                        query.snapshot = Some(snapshot.parse().ok()?);
                    }
                    Move::ScanWindow(offset) => {
                        query.page = NonZeroU64::MIN;
                        query.match_page = NonZeroU64::MIN;
                        query.scan_offset = Some(NonZeroU64::new(offset)?);
                        query.snapshot = None;
                    }
                }
                Some($variant(query))
            }};
        }
        match self {
            Self::Content(query) => moved!(query, AstSearchQuery::Match0),
            Self::Summary(query) => moved!(query, AstSearchQuery::MatchView),
        }
    }

    /// The query as a continuation copies it; an object rule keeps only the
    /// matchers the caller set.
    pub(super) fn to_value(self) -> Value {
        let mut value = match self {
            Self::Content(query) => serde_json::to_value(query),
            Self::Summary(query) => serde_json::to_value(query),
        }
        .unwrap_or_else(|_| json!({}));
        if let Some(rule) = value.get_mut("rule") {
            crate::response::result::remove_nulls(rule);
        }
        value
    }
}
