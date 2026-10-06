//! The typed astRewrite row and its rule-kind views.

use crate::tools::id::query_limits::ast_rewrite as limits;
use crate::tools::num::usize_of;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub use crate::contracts::tool_types::{
    ArPostconditionsItem, AstRewriteQuery, AstRewriteQueryPattern, AstRewriteQueryRule,
};

/// Binds `$field` from either rule kind of `$query`.
macro_rules! either_kind {
    ($query:expr, $field:ident => $value:expr) => {
        match $query {
            AstRewriteQuery::Pattern(AstRewriteQueryPattern { $field, .. })
            | AstRewriteQuery::Rule(AstRewriteQueryRule { $field, .. }) => $value,
        }
    };
}

/// Rule-kind-independent views over the generated wire query, with the
/// runtime's defaults for the optional bounds.
impl AstRewriteQuery {
    pub fn path(&self) -> &str {
        either_kind!(self, path => path.as_str())
    }
    /// The requested parser; `None` asks the runtime to infer it.
    pub fn language(&self) -> Option<&str> {
        either_kind!(self, language => language.as_ref().map(|language| language.as_str()))
    }
    pub fn pattern(&self) -> Option<&str> {
        match self {
            Self::Pattern(query) => Some(query.pattern.as_str()),
            Self::Rule(_) => None,
        }
    }
    pub fn rewrite(&self) -> Option<&str> {
        match self {
            Self::Pattern(query) => Some(query.rewrite.as_str()),
            Self::Rule(_) => None,
        }
    }
    pub fn default_excludes(&self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        either_kind!(self, default_excludes => default_excludes.defaults())
    }
    /// `include` entries as globs: a bare word matches names containing it.
    pub fn include(&self) -> Option<Vec<String>> {
        either_kind!(self, include => include
            .as_ref()
            .map(|globs| globs.iter().map(|glob| crate::policy::include::include_glob(glob)).collect()))
    }
    pub fn exclude(&self) -> Option<Vec<String>> {
        either_kind!(self, exclude => exclude
            .as_ref()
            .map(|globs| globs.iter().map(ToString::to_string).collect()))
    }
    pub fn apply(&self) -> bool {
        either_kind!(self, apply => apply.as_ref().is_some_and(|apply| apply.0))
    }
    pub fn debug(&self) -> bool {
        either_kind!(self, debug => *debug)
    }
    /// Expected pre-image hashes, in path order.
    pub fn expected_hashes(&self) -> Option<BTreeMap<String, String>> {
        either_kind!(self, expected_hashes => expected_hashes.as_ref().map(|hashes| {
            hashes
                .iter()
                .map(|(path, hash)| (path.to_string(), hash.to_string()))
                .collect()
        }))
    }
    pub fn selected_match_ids(&self) -> Option<Vec<String>> {
        either_kind!(self, selected_match_ids => selected_match_ids
            .as_ref()
            .map(|ids| ids.iter().map(ToString::to_string).collect()))
    }
    pub fn postconditions(&self) -> Option<&[ArPostconditionsItem]> {
        either_kind!(self, postconditions => postconditions.as_deref().map(Vec::as_slice))
    }
    pub fn max_files(&self) -> usize {
        either_kind!(self, max_files => max_files.as_ref().map_or_else(|| limits::MAX_FILES_DEFAULT, |n| usize_of(n.0)))
    }
    pub fn max_matches(&self) -> usize {
        either_kind!(self, max_matches => max_matches.as_ref().map_or_else(|| limits::MAX_MATCHES_DEFAULT, |n| usize_of(n.0)))
    }
    pub fn page(&self) -> usize {
        either_kind!(self, page => page.as_ref().map_or_else(|| limits::PAGE_DEFAULT, |n| usize_of(n.0)))
    }
    pub fn page_size(&self) -> usize {
        either_kind!(self, page_size => page_size.as_ref().map_or_else(|| limits::PAGE_SIZE_DEFAULT, |n| usize_of(n.0)))
    }
    pub fn snapshot(&self) -> Option<&str> {
        either_kind!(self, snapshot => snapshot.as_ref().map(|snapshot| snapshot.0.as_str()))
    }
}

/// A parsed astRewrite row: the generated query plus the parser inferred
/// when `language` is omitted. The runtime parses it with its shared
/// `parse_query`, so a shape mismatch has one code across tools.
#[derive(Clone, Debug)]
pub struct RewriteRequest {
    pub(super) query: AstRewriteQuery,
    /// Parser inferred when `language` is omitted (set before any scan).
    pub(super) inferred_lang: Option<String>,
}

impl<'de> Deserialize<'de> for RewriteRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let request = Self {
            query: AstRewriteQuery::deserialize(deserializer)?,
            inferred_lang: None,
        };
        // A YAML rule string must parse into a rule before any scan.
        request.rule_fields().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

/// Rule-file metadata a pasted ast-grep rule document may carry; none of it
/// affects matching (`language` selects the parser).
const RULE_FILE_METADATA: &[&str] = &[
    "id", "language", "severity", "message", "note", "url", "metadata",
];

/// A YAML rule string as the equivalent rule object: a bare rule, or a rule
/// file whose `rule:` is used (its metadata keys are ignored).
pub(super) fn rule_from_yaml(text: &str) -> Result<Value, String> {
    let value: Value =
        serde_yaml_ng::from_str(text).map_err(|error| format!("invalid rule YAML: {error}"))?;
    let Some(object) = value.as_object() else {
        return Err("rule YAML must be a mapping of ast-grep rule keys".to_owned());
    };
    let Some(rule) = object.get("rule") else {
        return Ok(value);
    };
    if let Some(key) = object
        .keys()
        .find(|key| *key != "rule" && !RULE_FILE_METADATA.contains(&key.as_str()))
    {
        return Err(format!(
            "rule file key `{key}` is not read from the rule string; pass it as its own astRewrite field"
        ));
    }
    Ok(rule.clone())
}

impl RewriteRequest {
    /// The parser this request runs with: `language`, else the inferred one.
    pub fn lang(&self) -> &str {
        self.query
            .language()
            .or(self.inferred_lang.as_deref())
            .unwrap_or_default()
    }
    /// The rule kind's ast-grep fields (`rule`, `constraints`, `utils`,
    /// `transform`, `fix`) as rule-config JSON, in key order; a YAML-string
    /// `rule` becomes the same object, so both shapes preview identically.
    fn rule_fields(&self) -> Result<Map<String, Value>, String> {
        let AstRewriteQuery::Rule(rule) = &self.query else {
            return Ok(Map::new());
        };
        let Value::Object(mut row) =
            serde_json::to_value(rule).map_err(|error| error.to_string())?
        else {
            return Ok(Map::new());
        };
        row.retain(|key, _| RULE_FIELDS.contains(&key.as_str()));
        row.values_mut()
            .for_each(crate::tools::result::remove_nulls);
        if let Some(Value::String(text)) = row.get("rule") {
            // The YAML rule takes the object rule's typed form, so both shapes
            // serialize (and digest) as one rule.
            let parsed = rule_from_yaml(text)?;
            let mut canonical =
                serde_json::from_value::<crate::contracts::tool_types::AstRule>(parsed.clone())
                    .ok()
                    .and_then(|rule| serde_json::to_value(rule).ok())
                    .unwrap_or(parsed);
            crate::tools::result::remove_nulls(&mut canonical);
            row.insert("rule".to_owned(), canonical);
        }
        Ok(row)
    }
    /// The rule kind's ast-grep fields; empty for a pattern.
    pub fn rule_config_fields(&self) -> Map<String, Value> {
        self.rule_fields().unwrap_or_default()
    }
}

/// The ast-grep rule-config fields of the rule kind.
const RULE_FIELDS: &[&str] = &["rule", "constraints", "utils", "transform", "fix"];

impl std::ops::Deref for RewriteRequest {
    type Target = AstRewriteQuery;
    fn deref(&self) -> &AstRewriteQuery {
        &self.query
    }
}
