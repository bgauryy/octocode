use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

use ast_grep_config::{GlobalRules, RuleConfig, SerializableRuleConfig};
use ast_grep_core::{
    language::Language,
    matcher::{Pattern, PatternBuilder, PatternError},
    meta_var::MetaVariable,
    replacer::Replacer,
    tree_sitter::{LanguageExt, StrDoc, TSLanguage},
};
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use crate::signatures::languages::{all_entries, LanguageEntry};

use super::language::{primary_expando_for_ext, AgLanguage};
use super::octo::parse_tree_with_deadline;

pub const MAX_REWRITE_CONTENT_BYTES: usize = 1_000_000;
const MAX_REWRITE_MATCHES: usize = 100_000;

/// The ast-grep rewrite adapter uses Octocode's canonical grammar registry
/// rather than a second bundled language inventory.
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
        self.octocode_language().preprocess_rewrite_pattern(query)
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
                        let text = std::str::from_utf8(bytes).map_err(|_| {
                            format!(
                                "[structural.rewrite.range] transformed capture {name} is not valid UTF-8"
                            )
                        })?;
                        captures.insert(
                            name,
                            StructuralRewriteCapture {
                                kind: "transformed".to_owned(),
                                texts: vec![text.to_owned()],
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
            replacement: String::from_utf8(replacement).map_err(|_| {
                "[structural.rewrite.range] generated replacement is not valid UTF-8".to_owned()
            })?,
            captures,
        });
    }
    Ok(output)
}

/// Count tree-sitter ERROR and MISSING nodes in `content` parsed as
/// `language_selector`. Rewrite staging compares the count before and after
/// splicing so a template that produces broken syntax is rejected instead of
/// committed; a boolean `has_error()` cannot distinguish pre-existing damage
/// from net-new damage.
pub fn count_syntax_errors(content: &str, language_selector: &str) -> Result<u32, String> {
    if content.len() > MAX_REWRITE_CONTENT_BYTES {
        return Err(format!(
            "[structural.content.tooLarge] structural rewrite content exceeds {MAX_REWRITE_CONTENT_BYTES} byte limit"
        ));
    }
    let language = RewriteLanguage::from_selector(language_selector).ok_or_else(|| {
        format!("[structural.rewrite.invalid] {language_selector} is not supported")
    })?;
    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
    let tree = parse_tree_with_deadline(&language.entry.language, content, deadline)
        .map_err(|error| format!("[{}] {}", error.code, error.message))?;
    let mut count = 0u32;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            count = count.saturating_add(1);
        }
        for index in (0..node.child_count()).rev() {
            if let Some(child) = node.child(index) {
                stack.push(child);
            }
        }
    }
    Ok(count)
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
    #[cfg(feature = "tree-sitter-scala")]
    fn scala_rewrite_uses_the_canonical_parser() {
        let found = rewrite(
            "val value = oldCall(foo)\n",
            json!({
                "id":"octocode-inline-rewrite",
                "language":"scala",
                "severity":"warning",
                "message":"Octocode inline structural rewrite",
                "rule":{"pattern":"oldCall($A)"},
                "fix":"newCall($A)"
            }),
        )
        .expect("Scala rewrite");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].replacement, "newCall(foo)");
    }

    #[test]
    fn every_first_class_language_family_rewrites_with_its_canonical_parser() {
        let mut cases = vec![
            (
                "javascript",
                "oldCall(foo);\n",
                "oldCall($A)",
                "newCall($A)",
            ),
            (
                "typescript",
                "oldCall(foo);\n",
                "oldCall($A)",
                "newCall($A)",
            ),
            ("python", "old_call(foo)\n", "old_call($A)", "new_call($A)"),
            (
                "rust",
                "fn main() { old_call(foo); }\n",
                "old_call($A)",
                "new_call($A)",
            ),
            (
                "go",
                "package main\nfunc main() { oldCall(foo) }\n",
                "oldCall($A)",
                "newCall($A)",
            ),
            (
                "java",
                "class Demo { void run() { oldCall(foo); } }\n",
                "oldCall($A)",
                "newCall($A)",
            ),
            (
                "c",
                "void run() { old_call(foo); }\n",
                "old_call($A);",
                "new_call($A);",
            ),
        ];
        #[cfg(feature = "tree-sitter-cpp")]
        cases.push((
            "cpp",
            "void run() { old_call(foo); }\n",
            "old_call($A);",
            "new_call($A);",
        ));
        #[cfg(feature = "tree-sitter-c-sharp")]
        cases.push((
            "csharp",
            "class Demo { void Run() { oldCall(foo); } }\n",
            "oldCall($A)",
            "newCall($A)",
        ));
        #[cfg(feature = "tree-sitter-scala")]
        cases.push((
            "scala",
            "object Demo { def run() = oldCall(foo) }\n",
            "oldCall($A)",
            "newCall($A)",
        ));

        for (language, source, pattern, fix) in cases {
            let found = rewrite(
                source,
                json!({
                    "id":"first-class-language-rewrite",
                    "language":language,
                    "rule":{"pattern":pattern},
                    "fix":fix
                }),
            )
            .unwrap_or_else(|error| panic!("{language}: {error}"));
            assert_eq!(found.len(), 1, "{language}");
            assert!(found[0].replacement.starts_with("new"), "{language}");
        }
    }

    #[test]
    fn syntax_error_count_distinguishes_broken_from_clean_source() {
        assert_eq!(
            count_syntax_errors("const value = call(foo);\n", "typescript").expect("clean"),
            0
        );
        assert!(
            count_syntax_errors("const value = call(foo;\n", "typescript").expect("broken") > 0
        );
        let error = count_syntax_errors("x", "ruby").expect_err("unsupported selector");
        assert!(error.contains("ruby is not supported"), "{error}");
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
