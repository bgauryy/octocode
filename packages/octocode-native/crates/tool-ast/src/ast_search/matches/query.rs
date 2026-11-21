use octocode_contract_types::AstSearchQuery;
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_contract_types::AstSearchQueryMatchView;
use octocode_contract_types::AstSearchSort;
use octocode_engine::num::saturating_u32;
use octocode_tools_shared::variant_field::non_empty;
use serde_json::Value;
use serde_json::json;
use std::num::NonZeroU64;

/// How a continuation of a match page moves its query: to the next page of
/// its matches, or past the files a scan window evaluated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Move {
    Page(u64),
    ScanWindow(u64),
}

/// A `match` query in its content or summary view.
#[derive(Clone, Copy, Debug)]
pub enum MatchQuery<'a> {
    Content(&'a AstSearchQueryMatch0),
    Summary(&'a AstSearchQueryMatchView),
}

/// A `match` query the runtime owns, in either form.
pub(super) enum OwnedMatch {
    Content(AstSearchQueryMatch0),
    Summary(AstSearchQueryMatchView),
}

impl OwnedMatch {
    pub(super) fn query(&self) -> MatchQuery<'_> {
        match self {
            Self::Content(query) => MatchQuery::Content(query),
            Self::Summary(query) => MatchQuery::Summary(query),
        }
    }
}

/// A pattern a rule spells, at any depth: the JSON pointer of the rule object
/// that holds it (`""` for the rule itself, `/has/all/0` nested), and its text
/// or `{context, selector}` object.
pub(super) struct RulePattern {
    pub(super) at: String,
    pub(super) pattern: Value,
}

impl RulePattern {
    /// Whether the pattern is under an odd number of `not`s: there a pattern
    /// that matches less excludes less, so it cannot empty a result.
    pub(super) fn negated(&self) -> bool {
        self.at
            .split('/')
            .filter(|segment| *segment == "not")
            .count()
            % 2
            == 1
    }

    /// The pattern as the engine reads it; `None` for a value that is
    /// neither text nor a `{context, selector}` object.
    pub(super) fn written(&self) -> Option<octocode_engine::structural::WrittenPattern<'_>> {
        use octocode_engine::structural::WrittenPattern;
        match &self.pattern {
            Value::String(text) => Some(WrittenPattern::Text(text)),
            pattern => Some(WrittenPattern::Context {
                context: pattern.get("context")?.as_str()?,
                selector: pattern.get("selector")?.as_str()?,
            }),
        }
    }
}

/// Each pattern of `rule` (a rule object, or a list or field inside one)
/// under the pointer `at`, in the rule's field order.
fn collect_patterns(rule: &Value, at: &str, found: &mut Vec<RulePattern>) {
    match rule {
        Value::Object(fields) => {
            for (key, value) in fields {
                if key == "pattern" {
                    if !value.is_null() {
                        found.push(RulePattern {
                            at: at.to_owned(),
                            pattern: value.clone(),
                        });
                    }
                    continue;
                }
                let key = key.replace('~', "~0").replace('/', "~1");
                collect_patterns(value, &format!("{at}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_patterns(item, &format!("{at}/{index}"), found);
            }
        }
        _ => {}
    }
}

/// Binds `$field` from whichever form `$query` is.
macro_rules! either_form {
    ($query:expr, $field:ident => $value:expr) => {
        octocode_tools_shared::variant_field::variant_field!($query, newtype [
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
    /// The file or directory the row searches: the one it names, else the
    /// workspace root; none for inline `content`.
    pub fn path(self) -> Option<String> {
        either_form!(self, path => path.as_ref().map(ToString::to_string)).or_else(|| {
            self.content()
                .is_none()
                .then(|| super::super::source::WALK_ROOT.to_owned())
        })
    }
    /// Inline source the row searches in place of a path.
    pub fn content(self) -> Option<&'a str> {
        either_form!(self, content => content.as_ref().map(|content| content.as_str()))
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
    /// Every pattern the rule spells, nested ones included ([`RulePattern`]).
    pub(super) fn patterns(self) -> Vec<RulePattern> {
        let mut found = Vec::new();
        if let Some(rule) = self.rule() {
            collect_patterns(&rule, "", &mut found);
        }
        found
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn default_excludes(self) -> bool {
        use octocode_runtime_base::policy::prune::DefaultsFlag;
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
        // The typed sort always holds its order: the key's own when omitted.
        either_form!(self, sort => Some(
            match sort {
                AstSearchSort::Ascending(named) => named.order,
                AstSearchSort::Descending(measure) => measure.order,
            } == octocode_contract_types::SortOrder::Desc
        ))
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
        either_form!(self, parser_globs => parser_globs.as_ref().map(octocode_tools_shared::ast_rule::parser_globs))
    }
    pub fn sort(self) -> Option<&'static str> {
        // Each form holds one key: names ascend, the match count descends.
        either_form!(self, sort => Some(match sort {
            AstSearchSort::Ascending(_) => "path",
            AstSearchSort::Descending(_) => "matchCount",
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

    /// This query moved to `to`, bound to the `snapshot` its pages share, or
    /// for a scan window to what the windows before it listed; `None` when
    /// `snapshot` is no valid token.
    pub(super) fn moved(self, to: Move, snapshot: &str) -> Option<AstSearchQuery> {
        macro_rules! moved {
            ($query:expr, $variant:path) => {{
                let mut query = $query.clone();
                match to {
                    Move::Page(page) => {
                        query.page = NonZeroU64::new(page)?;
                        query.snapshot = Some(snapshot.parse().ok()?);
                    }
                    Move::ScanWindow(offset) => {
                        query.page = NonZeroU64::MIN;
                        query.scan_offset = Some(NonZeroU64::new(offset)?);
                        query.snapshot = Some(snapshot.parse().ok()?);
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

    /// This query as its scan's first window asks it: no `scanOffset` and no
    /// pin of windows before it. A ranked scan's last window answers as this
    /// query ([`super::standing`]).
    pub(super) fn at_scan_start(self) -> OwnedMatch {
        macro_rules! started {
            ($query:expr, $variant:path) => {{
                let mut query = $query.clone();
                query.scan_offset = None;
                query.snapshot = None;
                $variant(query)
            }};
        }
        match self {
            Self::Content(query) => started!(query, OwnedMatch::Content),
            Self::Summary(query) => started!(query, OwnedMatch::Summary),
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
            octocode_runtime_base::response::result::remove_nulls(rule);
        }
        value
    }
}
