use std::collections::HashMap;

use ast_grep_config::{GlobalRules, RuleConfig, SerializableRuleConfig};
use ast_grep_core::{meta_var::MetaVariable, replacer::Replacer, tree_sitter::LanguageExt};
use ast_grep_language::SupportLang;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_REWRITE_CONTENT_BYTES: usize = 1_000_000;
const MAX_REWRITE_MATCHES: usize = 100_000;

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
    let serialized: SerializableRuleConfig<SupportLang> = serde_json::from_value(rule_config)
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
