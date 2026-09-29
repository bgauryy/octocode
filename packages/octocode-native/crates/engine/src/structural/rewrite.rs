use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use ast_grep_config::{Fixer, GlobalRules, RuleConfig, SerializableRuleConfig};
use ast_grep_core::{
    AstGrep,
    language::Language,
    matcher::{Pattern, PatternBuilder, PatternError},
    meta_var::MetaVariable,
    replacer::Replacer,
    tree_sitter::{LanguageExt, StrDoc, TSLanguage},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use crate::signatures::languages::{LanguageEntry, all_entries};
use crate::text::utf8_offsets::LineIndex;

use super::kinds::named_kind_id;
use super::language::{AgLanguage, primary_expando_for_ext};
use super::octo::parse_tree_with_deadline;

pub const MAX_REWRITE_CONTENT_BYTES: usize = crate::signatures::MAX_PARSE_SIZE;
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
        let entry = all_entries()
            .iter()
            .find(|entry| matches_selector(entry, selector))?;
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

fn matches_selector(entry: &LanguageEntry, selector: &str) -> bool {
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
}

pub(super) fn rewrite_language_extensions(selector: &str) -> Option<HashSet<&'static str>> {
    let mut extensions: HashSet<&'static str> = all_entries()
        .iter()
        .filter(|entry| matches_selector(entry, selector))
        .flat_map(|entry| entry.extensions.iter().copied())
        .collect();
    if extensions.is_empty() {
        return None;
    }
    if selector.eq_ignore_ascii_case("cpp") || selector.eq_ignore_ascii_case("c++") {
        extensions.insert("h");
    }
    Some(extensions)
}

/// The parser a rewrite uses for `path` under `selector`: the file's own
/// extension when it belongs to the selector's family (`.tsx` under
/// `typescript` needs the TSX grammar), `.h` as C++ under `cpp`, else the
/// selector. Scanning and syntax-regression checks must agree on this, or JSX
/// files are error-counted with a grammar that rejects JSX.
pub fn rewrite_parser_for_path(selector: &str, path: &str) -> String {
    let Some(extension) = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return selector.to_owned();
    };
    let is_cpp = selector.eq_ignore_ascii_case("cpp") || selector.eq_ignore_ascii_case("c++");
    if extension == "h" && is_cpp {
        return "cpp".to_owned();
    }
    match rewrite_language_extensions(selector) {
        Some(extensions) if extensions.contains(extension.as_str()) => extension,
        _ => selector.to_owned(),
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

    /// `0` makes ast-grep reject kinds no parsed node can carry.
    fn kind_to_id(&self, kind: &str) -> u16 {
        named_kind_id(&self.entry.language, kind).unwrap_or(0)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.get_ts_language()
            .field_id_for_name(field)
            .map(std::num::NonZero::get)
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
    compile_rewrite(rule_config)?.run(content)
}

/// A validated inline rule plus its single fixer. Compile once per request and
/// language, then `run` it on every file: deserializing and compiling the rule
/// per file dominated multi-file rewrites.
pub struct CompiledRewrite {
    config: RuleConfig<RewriteLanguage>,
    fixer: Fixer,
}

pub fn compile_rewrite(rule_config: Value) -> Result<CompiledRewrite, String> {
    let serialized: SerializableRuleConfig<RewriteLanguage> =
        serde_json::from_value(rule_config)
            .map_err(|error| format!("[structural.rewrite.invalid] {error}"))?;
    let config = RuleConfig::try_from(serialized, &GlobalRules::default()).map_err(|error| {
        format!(
            "[structural.rewrite.invalid] {}",
            rule_config_message(&error)
        )
    })?;
    let mut fixers = config
        .get_fixer()
        .map_err(|error| format!("[structural.rewrite.invalid] {error}"))?;
    if fixers.len() != 1 {
        return Err("[structural.rewrite.invalid] exactly one fixer is required".to_owned());
    }
    let fixer = fixers
        .pop()
        .ok_or_else(|| "[structural.rewrite.invalid] a fixer is required".to_owned())?;
    Ok(CompiledRewrite { config, fixer })
}

/// A rule-compile error with its cause. ast-grep wraps every rule-core error
/// as "Fail to parse yaml as Rule." (even for a plain pattern) and keeps the
/// real reason, such as an undefined metavariable, in the source chain.
fn rule_config_message(error: &ast_grep_config::RuleConfigError) -> String {
    use ast_grep_config::{RuleConfigError, RuleCoreError};
    match error {
        RuleConfigError::Core(RuleCoreError::UndefinedMetaVar(name, section)) => {
            let section = if *section == "fix" {
                "rewrite"
            } else {
                section
            };
            format!(
                "Undefined metavariable `${name}` used in `{section}`: every `$NAME` there must be captured by the pattern or rule."
            )
        }
        RuleConfigError::Core(core) => core.to_string(),
        other => {
            let mut message = other.to_string();
            let mut source = std::error::Error::source(other);
            while let Some(cause) = source {
                message.push_str(": ");
                message.push_str(&cause.to_string());
                source = cause.source();
            }
            message
        }
    }
}

/// Rewrite matches plus the ERROR/MISSING node count of the same parse, so a
/// caller that needs both never parses the content twice.
#[derive(Debug)]
pub struct RewriteScan {
    pub matches: Vec<StructuralRewriteMatch>,
    pub syntax_errors: u32,
}

/// When [`CompiledRewrite`] counts syntax errors on its parse tree.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CountErrors {
    /// Only when the content matched (a file-tree scan skips non-matches).
    WhenMatched,
    Always,
}

impl CompiledRewrite {
    pub fn run(&self, content: &str) -> Result<Vec<StructuralRewriteMatch>, String> {
        self.scan_before(
            content,
            Instant::now() + AST_EXECUTION_TIMEOUT,
            CountErrors::WhenMatched,
        )
        .map(|scan| scan.matches)
    }

    /// One parse: the rewrite matches and the tree's syntax-error count.
    pub fn scan(&self, content: &str) -> Result<RewriteScan, String> {
        self.scan_before(
            content,
            Instant::now() + AST_EXECUTION_TIMEOUT,
            CountErrors::Always,
        )
    }

    pub(super) fn scan_with(
        &self,
        content: &str,
        count: CountErrors,
    ) -> Result<RewriteScan, String> {
        self.scan_before(content, Instant::now() + AST_EXECUTION_TIMEOUT, count)
    }

    /// Parses with the shared deadline-bound parser (ast-grep's own parse has no
    /// cancellation) and re-checks the deadline between matches.
    fn scan_before(
        &self,
        content: &str,
        deadline: Instant,
        count: CountErrors,
    ) -> Result<RewriteScan, String> {
        if content.len() > MAX_REWRITE_CONTENT_BYTES {
            return Err(format!(
                "[structural.content.tooLarge] structural rewrite content exceeds {MAX_REWRITE_CONTENT_BYTES} byte limit"
            ));
        }
        let config = &self.config;
        let fixer = &self.fixer;
        let tree = parse_tree_with_deadline(&config.language.entry.language, content, deadline)
            .map_err(|error| format!("[{}] {}", error.code, error.message))?;
        // `Tree::clone` is a reference-counted copy, not a re-parse.
        let error_tree = tree.clone();
        let grep = AstGrep::doc(StrDoc {
            src: content.to_owned(),
            lang: config.language.clone(),
            tree,
        });
        let line_index = LineIndex::new(content);
        let mut output = Vec::new();
        for matched in grep.root().find_all(&config.matcher) {
            if Instant::now() >= deadline {
                return Err(
                    "[structural.rewrite.interrupted] structural rewrite exceeded its execution deadline"
                        .to_owned(),
                );
            }
            if output.len() >= MAX_REWRITE_MATCHES {
                return Err(format!(
                    "[structural.rewrite.matchLimit] structural rewrite exceeds {MAX_REWRITE_MATCHES} matches"
                ));
            }
            let replaced = fixer.get_replaced_range(&matched, &config.matcher);
            let replacement = fixer.generate_replacement(&matched);
            // Columns are 0-based UTF-16 code units like astSearch and LSP
            // (ast-grep's `column()` counts Unicode scalars).
            let span = matched.range();
            let position = |byte: usize| {
                u32::try_from(byte)
                    .map(|byte| line_index.byte_to_position(byte))
                    .map_err(|_| "[structural.rewrite.range] byte offset overflow".to_owned())
            };
            let (start_line, start_column) = position(span.start)?;
            let (end_line, end_column) = position(span.end)?;
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
                        line: start_line,
                        column: start_column,
                    },
                    end: StructuralRewritePosition {
                        line: end_line,
                        column: end_column,
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
        let syntax_errors = if count == CountErrors::Always || !output.is_empty() {
            count_tree_errors(&error_tree)
        } else {
            0
        };
        Ok(RewriteScan {
            matches: output,
            syntax_errors,
        })
    }
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
    Ok(count_tree_errors(&tree))
}

/// ERROR and MISSING nodes in a parsed tree.
fn count_tree_errors(tree: &tree_sitter::Tree) -> u32 {
    // Preorder cursor walk: O(n), no recursion, no per-node child indexing.
    let mut count = 0u32;
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            count = count.saturating_add(1);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return count;
            }
        }
    }
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

    /// Columns are UTF-16 code units (astSearch/LSP), not Unicode scalars:
    /// an astral emoji counts 2, a BMP accent 1.
    #[test]
    fn columns_are_utf16_code_units_after_non_ascii_text() {
        let found = rewrite(
            "const s = \"😀é\"; oldCall(foo);\n",
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
        // `const s = "` = 11 units, 😀 = 2, é = 1, `"; ` = 3 → 17.
        assert_eq!(found[0].range.start.line, 0);
        assert_eq!(found[0].range.start.column, 17);
        assert_eq!(found[0].range.end.column, 17 + "oldCall(foo)".len() as u32);
    }

    #[test]
    fn scan_reports_matches_and_syntax_errors_from_one_parse() {
        let compiled = compile_rewrite(json!({
            "id":"octocode-inline-rewrite",
            "language":"typescript",
            "severity":"warning",
            "message":"Octocode inline structural rewrite",
            "rule":{"pattern":"oldCall($A)"},
            "fix":"newCall($A)"
        }))
        .expect("compile");
        let clean = compiled.scan("oldCall(a);\n").expect("scan");
        assert_eq!((clean.matches.len(), clean.syntax_errors), (1, 0));
        let broken = "oldCall(a);\nconst = ;\n";
        let damaged = compiled.scan(broken).expect("scan");
        assert_eq!(damaged.matches.len(), 1);
        assert_eq!(
            damaged.syntax_errors,
            count_syntax_errors(broken, "typescript").expect("count")
        );
        assert!(damaged.syntax_errors > 0);
        // No matches still reports errors when asked to count always.
        assert!(compiled.scan("const = ;\n").expect("scan").syntax_errors > 0);
        let skipped = compiled
            .scan_with("const = ;\n", CountErrors::WhenMatched)
            .expect("scan");
        assert_eq!(skipped.syntax_errors, 0);
    }

    #[test]
    fn undefined_rewrite_metavariable_is_named_not_reported_as_yaml() {
        let error = rewrite(
            "fillGoal(next, goal);\n",
            json!({
                "id":"octocode-inline-rewrite",
                "language":"typescript",
                "rule":{"pattern":"fillGoal($A, $B)"},
                "fix":"fillGoal($C, $B)"
            }),
        )
        .expect_err("undefined metavariable");
        assert!(error.contains("structural.rewrite.invalid"), "{error}");
        assert!(error.contains("`$C`"), "{error}");
        assert!(!error.contains("yaml"), "{error}");
    }

    fn kind_rule(kind: &str) -> Value {
        json!({
            "id":"octocode-inline-rewrite",
            "language":"typescript",
            "severity":"warning",
            "message":"Octocode inline structural rewrite",
            "rule":{"kind":kind},
            "fix":"x"
        })
    }

    #[test]
    fn kind_names_that_never_occur_are_rejected_not_silently_empty() {
        // `expression` is a hidden supertype: it resolves to a non-zero id that no
        // node carries, so the rule would compile and match nothing.
        let error = rewrite("const a = b + c;\n", kind_rule("expression")).expect_err("supertype");
        assert!(error.contains("structural.rewrite.invalid"), "{error}");
        // Any prefix of "ERROR" resolves to the ERROR symbol inside tree-sitter.
        let error = rewrite("const a = 1;\n", kind_rule("ERR")).expect_err("error prefix");
        assert!(error.contains("structural.rewrite.invalid"), "{error}");
        // Real kinds, including ERROR itself, still compile.
        assert_eq!(
            rewrite("const a = b + c;\n", kind_rule("binary_expression"))
                .expect("kind")
                .len(),
            1
        );
        assert!(rewrite("const a = 1;\n", kind_rule("ERROR")).is_ok());
    }

    #[test]
    fn compiled_rewrite_is_shareable_and_reusable() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CompiledRewrite>();
        let compiled = compile_rewrite(json!({
            "id":"octocode-inline-rewrite",
            "language":"typescript",
            "severity":"warning",
            "message":"Octocode inline structural rewrite",
            "rule":{"pattern":"oldCall($A)"},
            "fix":"newCall($A)"
        }))
        .expect("compile");
        assert_eq!(compiled.run("oldCall(a);\n").expect("first").len(), 1);
        assert_eq!(
            compiled
                .run("oldCall(b); oldCall(c);\n")
                .expect("second")
                .len(),
            2
        );
    }

    #[test]
    fn rewrite_honors_an_expired_deadline() {
        let compiled = compile_rewrite(kind_rule("identifier")).expect("compile");
        let error = compiled
            .scan_before("const a = b;\n", Instant::now(), CountErrors::WhenMatched)
            .expect_err("expired deadline");
        assert!(error.contains("interrupted"), "{error}");
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

    #[test]
    fn parser_for_path_follows_the_file_extension_within_the_family() {
        assert_eq!(rewrite_parser_for_path("typescript", "src/App.tsx"), "tsx");
        assert_eq!(rewrite_parser_for_path("typescript", "src/a.ts"), "ts");
        assert_eq!(rewrite_parser_for_path("cpp", "include/w.h"), "cpp");
        assert_eq!(rewrite_parser_for_path("c", "include/w.h"), "h");
        // A nonstandard suffix keeps the explicit selector.
        assert_eq!(
            rewrite_parser_for_path("typescript", "script.foo"),
            "typescript"
        );
        assert_eq!(
            rewrite_parser_for_path("typescript", "Makefile"),
            "typescript"
        );
        // JSX counted with the TSX grammar is clean; with TS it is broken.
        let jsx = "export const App = () => <div>hi</div>;\n";
        let parser = rewrite_parser_for_path("typescript", "App.tsx");
        assert_eq!(count_syntax_errors(jsx, &parser).expect("tsx"), 0);
        assert!(count_syntax_errors(jsx, "typescript").expect("ts") > 0);
    }
}
