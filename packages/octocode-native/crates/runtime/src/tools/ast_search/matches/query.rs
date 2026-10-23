use crate::num::u32_of;
use octocode_contract_types::AstSearchQueryMatchPattern;
use octocode_contract_types::AstSearchQueryMatchRule;
use serde_json::Value;
use serde_json::json;

/// A `match` query in either of its generated forms (pattern or rule).
#[derive(Clone, Copy, Debug)]
pub enum MatchQuery<'a> {
    Pattern(&'a AstSearchQueryMatchPattern),
    Rule(&'a AstSearchQueryMatchRule),
}

/// Binds `$field` from whichever form `$query` is.
macro_rules! either_form {
    ($query:expr, $field:ident => $value:expr) => {
        crate::tools::variant_field::variant_field!($query, newtype [
            MatchQuery::Pattern => AstSearchQueryMatchPattern,
            MatchQuery::Rule => AstSearchQueryMatchRule,
        ] $field => $value)
    };
}

/// The wire rule as the JSON object the engine decodes. The typed rule holds
/// only serializable data, so it always converts.
fn to_rule_value(rule: &octocode_contract_types::AstSearchRule) -> Value {
    serde_json::to_value(rule).unwrap_or(Value::Null)
}

fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// Form-independent views in the engine's units.
impl<'a> MatchQuery<'a> {
    pub fn path(self) -> String {
        either_form!(self, path => path.to_string())
    }
    pub fn scope(self) -> Option<&'a octocode_contract_types::AstMatchScope> {
        either_form!(self, scope => scope.as_ref())
    }
    pub fn pattern(self) -> Option<String> {
        match self {
            Self::Pattern(query) => Some(query.pattern.to_string()),
            Self::Rule(_) => None,
        }
    }
    /// The typed rule as the engine's JSON rule object.
    pub fn rule(self) -> Option<Value> {
        match self {
            Self::Pattern(_) => None,
            Self::Rule(query) => Some(to_rule_value(&query.rule)),
        }
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include).map(|globs| crate::policy::include::include_globs(&globs)))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn default_excludes(self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        either_form!(self, default_excludes => default_excludes.defaults())
    }
    pub fn include_hidden(self) -> Option<bool> {
        either_form!(self, include_hidden => *include_hidden)
    }
    pub fn include_ignored(self) -> Option<bool> {
        either_form!(self, include_ignored => *include_ignored)
    }
    pub fn reverse(self) -> Option<bool> {
        either_form!(self, reverse => *reverse)
    }
    pub fn capture_text(self) -> Option<bool> {
        either_form!(self, capture_text => *capture_text)
    }
    /// Levels below `path` (1 = its children), as the wire counts them.
    pub fn max_depth(self) -> Option<u32> {
        either_form!(self, max_depth => max_depth.map(|depth| u32_of(depth.get())))
    }
    /// The caller's file scan cap; unset scans the whole scope within the
    /// scan time budget.
    pub fn max_files(self) -> Option<u32> {
        either_form!(self, max_files => max_files.map(|files| u32_of(files.get())))
    }
    /// Leading candidates (walk order) an earlier window evaluated.
    pub fn scan_offset(self) -> u32 {
        either_form!(self, scan_offset => scan_offset.map_or(0, |offset| u32_of(offset.get())))
    }
    /// The caller's matches per file page; unset fills the response window.
    pub fn match_page_size(self) -> Option<u32> {
        either_form!(self, match_page_size => match_page_size.map(|size| u32_of(size.get())))
    }
    pub fn match_content_length(self) -> Option<u32> {
        either_form!(self, match_content_length => Some(u32_of(*match_content_length)))
    }
    pub fn language(self) -> Option<String> {
        either_form!(self, language => language.as_ref().map(ToString::to_string))
    }
    pub fn language_globs(self) -> Option<&'a std::collections::BTreeMap<String, Vec<String>>> {
        either_form!(self, language_globs => (!language_globs.is_empty()).then_some(language_globs))
    }
    pub fn sort(self) -> Option<String> {
        either_form!(self, sort => Some(sort.to_string()))
    }
    pub fn result_view(self) -> Option<String> {
        either_form!(self, result_view => Some(result_view.to_string()))
    }
    pub fn page(self) -> u32 {
        either_form!(self, page => u32_of(*page))
    }
    pub fn match_page(self) -> u32 {
        either_form!(self, match_page => u32_of(*match_page))
    }
    /// The caller's files per page; unset fills the response window.
    pub fn page_size(self) -> Option<u32> {
        either_form!(self, page_size => page_size.map(|size| u32_of(size.get())))
    }
    pub fn snapshot(self) -> Option<String> {
        either_form!(self, snapshot => snapshot.as_ref().map(ToString::to_string))
    }
    /// The query as a continuation copies it; an object rule keeps only the
    /// matchers the caller set.
    pub(super) fn to_value(self) -> Value {
        let mut value = match self {
            Self::Pattern(query) => serde_json::to_value(query),
            Self::Rule(query) => serde_json::to_value(query),
        }
        .unwrap_or_else(|_| json!({}));
        if let Some(rule) = value.get_mut("rule") {
            crate::response::result::remove_nulls(rule);
        }
        value
    }
}
