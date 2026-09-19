use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use ast_grep_config::{GlobalRules, RuleConfig, SerializableRuleConfig};
use ast_grep_core::{
    language::Language,
    matcher::{Pattern, PatternBuilder, PatternError},
    meta_var::MetaVariable,
    replacer::Replacer,
    tree_sitter::{LanguageExt, StrDoc, TSLanguage, TSRange},
    Node,
};
use ast_grep_language::Html;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::signatures::languages::{all_entries, LanguageEntry};

use super::language::{primary_expando_for_ext, AgLanguage};

const MAX_REWRITE_CONTENT_BYTES: usize = 1_000_000;
const MAX_REWRITE_MATCHES: usize = 100_000;

/// The ast-grep rewrite adapter uses Octocode's canonical grammar registry
/// instead of `ast-grep-language::SupportLang`. The upstream enum enables its
/// complete built-in parser set by default, which linked unsupported grammars
/// and a second Kotlin parser into every release artifact.
#[derive(Clone)]
struct RewriteLanguage {
    entry: &'static LanguageEntry,
    extension: &'static str,
}

impl RewriteLanguage {
    fn from_selector(selector: &str) -> Option<Self> {
        let entry = all_entries().iter().find(|entry| {
            entry.name.eq_ignore_ascii_case(selector)
                || entry
                    .language_id
                    .is_some_and(|id| id.eq_ignore_ascii_case(selector))
                || entry
                    .selector_aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(selector))
                || entry
                    .extensions
                    .iter()
                    .any(|ext| ext.eq_ignore_ascii_case(selector))
        })?;
        let extension = entry
            .extensions
            .iter()
            .copied()
            .find(|ext| ext.eq_ignore_ascii_case(selector))
            .unwrap_or(entry.extensions[0]);
        Some(Self { entry, extension })
    }

    fn octocode_language(&self) -> AgLanguage {
        AgLanguage::new(self.extension, self.entry)
    }
}

impl fmt::Debug for RewriteLanguage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.entry.name)
    }
}

impl<'de> Deserialize<'de> for RewriteLanguage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let selector = String::deserialize(deserializer)?;
        Self::from_selector(&selector)
            .ok_or_else(|| de::Error::custom(format!("{selector} is not supported")))
    }
}

impl Language for RewriteLanguage {
    fn pre_process_pattern<'query>(&self, query: &'query str) -> Cow<'query, str> {
        self.octocode_language().preprocess_pattern(query)
    }

    fn expando_char(&self) -> char {
        primary_expando_for_ext(self.extension)
    }

    fn kind_to_id(&self, kind: &str) -> u16 {
        self.get_ts_language().id_for_node_kind(kind, true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.get_ts_language()
            .field_id_for_name(field)
            .map(|id| id.get())
    }

    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|source| StrDoc::try_new(source, self.clone()))
    }
}

impl LanguageExt for RewriteLanguage {
    fn get_ts_language(&self) -> TSLanguage {
        self.entry.language.clone()
    }

    fn extract_injections<L: LanguageExt>(
        &self,
        root: Node<StrDoc<L>>,
    ) -> Vec<(String, Vec<TSRange>)> {
        if self.entry.name == "HTML" {
            Html.extract_injections(root)
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralRewritePosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralRewriteRange {
    pub start: StructuralRewritePosition,
    pub end: StructuralRewritePosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralRewriteCapture {
    pub kind: String,
    pub texts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralRewriteMatch {
    pub byte_start: u32,
    pub byte_end: u32,
    pub range: StructuralRewriteRange,
    pub text: String,
    pub replaced_text: String,
    pub replacement: String,
    pub captures: HashMap<String, StructuralRewriteCapture>,
}

/// Compile and execute the canonical ast-grep inline-rule contract in process.
/// `rule_config` is the complete inline rule object, including language, rule,
/// constraints, utils, transforms, fix, and optional rewriters.
pub fn rewrite(content: &str, rule_config: Value) -> Result<Vec<StructuralRewriteMatch>, String> {
    if content.len() > MAX_REWRITE_CONTENT_BYTES {
        return Err(format!(
            "[structural.content.tooLarge] structural rewrite content exceeds {MAX_REWRITE_CONTENT_BYTES} byte limit"
        ));
    }
    let serialized: SerializableRuleConfig<RewriteLanguage> =
        serde_json::from_value(rule_config)
            .map_err(|error| format!("[structural.rewrite.invalid] {error}"))?;
    let config = RuleConfig::try_from(serialized, &GlobalRules::default())
        .map_err(|error| format!("[structural.rewrite.invalid] {error}"))?;
    let mut fixers = config
        .get_fixer()
        .map_err(|error| format!("[structural.rewrite.invalid] {error}"))?;
    if fixers.len() != 1 {
        return Err("[structural.rewrite.invalid] exactly one fixer is required".to_owned());
    }
    let fixer = fixers
        .pop()
        .ok_or_else(|| "[structural.rewrite.invalid] a fixer is required".to_owned())?;
    let grep = config.language.ast_grep(content);
    let mut output = Vec::new();
    for matched in grep.root().find_all(&config.matcher) {
        if output.len() >= MAX_REWRITE_MATCHES {
            return Err(format!(
                "[structural.rewrite.matchLimit] structural rewrite exceeds {MAX_REWRITE_MATCHES} matches"
            ));
        }
        let replaced = fixer.get_replaced_range(&matched, &config.matcher);
        let replacement = fixer.generate_replacement(&matched);
        let start = matched.start_pos();
        let end = matched.end_pos();
        let mut captures = HashMap::new();
        let env = matched.get_env();
        for variable in env.get_matched_variables() {
            match variable {
                MetaVariable::Capture(name, _) => {
                    if let Some(node) = env.get_match(&name) {
                        captures.insert(
                            name,
                            StructuralRewriteCapture {
                                kind: "single".to_owned(),
                                texts: vec![node.text().into_owned()],
                            },
                        );
                    } else if let Some(bytes) = env.get_transformed(&name) {
                        captures.insert(
                            name,
                            StructuralRewriteCapture {
                                kind: "transformed".to_owned(),
                                texts: vec![String::from_utf8_lossy(bytes).into_owned()],
                            },
                        );
                    }
                }
                MetaVariable::MultiCapture(name) => {
                    captures.insert(
                        name.clone(),
                        StructuralRewriteCapture {
                            kind: "multi".to_owned(),
                            texts: env
                                .get_multiple_matches(&name)
                                .into_iter()
                                .map(|node| node.text().into_owned())
                                .collect(),
                        },
                    );
                }
                _ => {}
            }
        }
        output.push(StructuralRewriteMatch {
            byte_start: u32::try_from(replaced.start)
                .map_err(|_| "[structural.rewrite.range] byte offset overflow".to_owned())?,
            byte_end: u32::try_from(replaced.end)
                .map_err(|_| "[structural.rewrite.range] byte offset overflow".to_owned())?,
            range: StructuralRewriteRange {
                start: StructuralRewritePosition {
                    line: u32::try_from(start.line())
                        .map_err(|_| "[structural.rewrite.range] line overflow".to_owned())?,
                    column: u32::try_from(start.column(&matched))
                        .map_err(|_| "[structural.rewrite.range] column overflow".to_owned())?,
                },
                end: StructuralRewritePosition {
                    line: u32::try_from(end.line())
                        .map_err(|_| "[structural.rewrite.range] line overflow".to_owned())?,
                    column: u32::try_from(end.column(&matched))
                        .map_err(|_| "[structural.rewrite.range] column overflow".to_owned())?,
                },
            },
            text: matched.text().into_owned(),
            replaced_text: content
                .get(replaced.clone())
                .ok_or_else(|| {
                    "[structural.rewrite.range] replacement range is not valid UTF-8".to_owned()
                })?
                .to_owned(),
            replacement: String::from_utf8_lossy(&replacement).into_owned(),
            captures,
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn pattern_rules_generate_ast_grep_replacements_and_captures() {
        let found = rewrite(
            "const value = oldCall(foo);\n",
            json!({
                "id":"octocode-inline-rewrite",
                "language":"typescript",
                "severity":"warning",
                "message":"Octocode inline structural rewrite",
                "rule":{"pattern":"oldCall($A)"},
                "fix":"newCall($A)"
            }),
        )
        .expect("rewrite");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "oldCall(foo)");
        assert_eq!(found[0].replacement, "newCall(foo)");
        assert_eq!(found[0].byte_start, 14);
        assert_eq!(found[0].captures["A"].texts, ["foo"]);
    }

    #[test]
    fn kotlin_rewrite_uses_the_canonical_parser() {
        let found = rewrite(
            "val value = oldCall(foo)\n",
            json!({
                "id":"octocode-inline-rewrite",
                "language":"kotlin",
                "severity":"warning",
                "message":"Octocode inline structural rewrite",
                "rule":{"pattern":"oldCall($A)"},
                "fix":"newCall($A)"
            }),
        )
        .expect("Kotlin rewrite");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].replacement, "newCall(foo)");
    }

    #[test]
    fn removed_ruby_parser_is_rejected() {
        let error = rewrite(
            "old_call(foo)\n",
            json!({
                "id":"octocode-inline-rewrite",
                "language":"ruby",
                "severity":"warning",
                "message":"Octocode inline structural rewrite",
                "rule":{"pattern":"old_call($A)"},
                "fix":"new_call($A)"
            }),
        )
        .expect_err("Ruby must not resolve through a hidden ast-grep parser");
        assert!(error.contains("ruby is not supported"), "{error}");
    }

    #[test]
    fn transformations_and_rewriters_use_upstream_semantics() {
        let found = rewrite(
            "const value = oldCall(fooBar);\n",
            json!({
                "id":"octocode-inline-rewrite",
                "language":"typescript",
                "rule":{"pattern":"oldCall($A)"},
                "transform":{
                    "OUT":{"rewrite":{"source":"$A","rewriters":["rename"]}}
                },
                "fix":"newCall($OUT)",
                "rewriters":[{
                    "id":"rename",
                    "rule":{"kind":"identifier"},
                    "fix":"renamed"
                }]
            }),
        )
        .expect("rewrite");
        assert_eq!(found[0].replacement, "newCall(renamed)");
        assert_eq!(found[0].captures["OUT"].kind, "transformed");
    }
}
