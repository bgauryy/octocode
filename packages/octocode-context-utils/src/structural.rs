//! Structural (AST) search over the tree-sitter grammars we already link.
//!
//! This is octocode's L2 search layer: it answers shape questions ripgrep
//! can't (a call shaped `foo($X)`, an `eval()` call site that is NOT inside a
//! comment/string) and that LSP is too heavy for. The matcher is
//! `ast-grep-core`; the grammars are the exact `tree_sitter::Language` values
//! in [`crate::signatures::languages`] — no second grammar set, no link
//! collision.
//!
//! Two query surfaces, mirroring ast-grep:
//!   * `pattern` — a code-shaped pattern (`console.log($$$)`). Metavars: `$X`
//!     is one node, `$$$ARGS` is a list.
//!   * `rule` — a YAML relational/composite rule object (`not`/`has`/`inside`/
//!     `all`/`any`), the only surface that can express negation and
//!     parent/child relationships.

use std::borrow::Cow;
use std::collections::HashMap;

use napi_derive::napi;

use ast_grep_config::{from_str, DeserializeEnv, SerializableRuleCore};
use ast_grep_core::language::Language;
use ast_grep_core::matcher::{PatternBuilder, PatternError};
use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc, TSLanguage};
use ast_grep_core::{NodeMatch, Pattern};

use crate::signatures::languages;

/// One structural match. Line numbers are 1-based so `start_line` can be fed
/// directly as an `lspGetSemantics` `lineHint`; columns are 0-based char
/// offsets (tree-sitter native).
#[napi(object)]
pub struct StructuralMatch {
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub text: String,
    /// Captured metavariables. `$X` yields a single-element list;
    /// `$$$ARGS` yields the full list of captured nodes. Keyed by the bare
    /// metavar name (no leading `$`).
    pub metavars: HashMap<String, Vec<String>>,
}

/// A tree-sitter language wrapped so ast-grep can drive it. A single wrapper
/// covers every grammar — the only per-language knob is `expando_char`, the
/// stand-in identifier char used while parsing a pattern in languages where
/// `$` is not a valid identifier character (Rust/Go/Python/C/…). Values match
/// the `ast-grep-language` crate so pattern semantics are identical (KPI #2).
#[derive(Clone)]
struct AgLanguage {
    ts: TSLanguage,
    expando: char,
}

impl Language for AgLanguage {
    fn kind_to_id(&self, kind: &str) -> u16 {
        self.ts.id_for_node_kind(kind, /* named */ true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.ts.field_id_for_name(field).map(|f| f.get())
    }

    fn expando_char(&self) -> char {
        self.expando
    }

    fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str> {
        pre_process_pattern(self.expando, query)
    }

    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|src| StrDoc::try_new(src, self.clone()))
    }
}

impl LanguageExt for AgLanguage {
    fn get_ts_language(&self) -> TSLanguage {
        self.ts.clone()
    }
}

/// The stand-in identifier char for `$` metavariables, per language. Mirrors
/// `ast-grep-language`: languages where `$` is a legal identifier char
/// (JS/TS/Java/Bash) keep `$`; the rest get a char the grammar accepts.
fn expando_for_ext(ext: &str) -> char {
    match ext {
        // `$` is a valid identifier character → no substitution needed.
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "java" | "sh" | "bash" | "zsh" => '$',
        // C / C++ accept this CJK code point as an identifier start.
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => '\u{10000}',
        // Go / Rust / Python / C# accept the micro sign as an identifier char.
        _ => '\u{00b5}',
    }
}

/// Verbatim port of `ast-grep-language::pre_process_pattern`: rewrites the
/// `$` sigil of capturing/anonymous-multiple metavars to the language's
/// expando char so the tree-sitter parser accepts the pattern. Literal `$`
/// (e.g. a non-metavar `$` in the source) is preserved.
fn pre_process_pattern(expando: char, query: &str) -> Cow<'_, str> {
    let mut ret = Vec::with_capacity(query.len());
    let mut dollar_count = 0;
    for c in query.chars() {
        if c == '$' {
            dollar_count += 1;
            continue;
        }
        let need_replace = matches!(c, 'A'..='Z' | '_') // $A or $$A or $$$A
            || dollar_count == 3; // anonymous multiple
        let sigil = if need_replace { expando } else { '$' };
        ret.extend(std::iter::repeat_n(sigil, dollar_count));
        dollar_count = 0;
        ret.push(c);
    }
    // trailing anonymous multiple
    let sigil = if dollar_count == 3 { expando } else { '$' };
    ret.extend(std::iter::repeat_n(sigil, dollar_count));
    Cow::Owned(ret.into_iter().collect())
}

/// Convert one ast-grep match into the napi-facing struct, pulling every
/// captured metavar out of the match environment. `$X` → single (one-element
/// list via `get_match`); `$$$ARGS` → list (via `get_multiple_matches`).
fn to_match(m: &NodeMatch<StrDoc<AgLanguage>>) -> StructuralMatch {
    let node = m.get_node();
    let start = node.start_pos();
    let end = node.end_pos();

    let env = m.get_env();
    let mut metavars: HashMap<String, Vec<String>> = HashMap::new();
    for var in env.get_matched_variables() {
        match var {
            MetaVariable::Capture(id, _) => {
                if let Some(matched) = env.get_match(&id) {
                    metavars.insert(id, vec![matched.text().to_string()]);
                }
            }
            MetaVariable::MultiCapture(id) => {
                let texts = env
                    .get_multiple_matches(&id)
                    .iter()
                    .map(|n| n.text().to_string())
                    .collect();
                metavars.insert(id, texts);
            }
            // Dropped (`$_`) / anonymous Multiple (`$$$`) are not captured.
            _ => {}
        }
    }

    StructuralMatch {
        start_line: (start.line() as u32) + 1,
        end_line: (end.line() as u32) + 1,
        start_col: start.column(node) as u32,
        end_col: end.column(node) as u32,
        text: node.text().to_string(),
        metavars,
    }
}

/// Run a structural search over `content`, parsed with the grammar resolved
/// from `ext`. Exactly one of `pattern` / `rule` must be `Some`.
///
/// Returns `Err` for: an unsupported extension, an invalid pattern, invalid
/// rule YAML, or both/neither query supplied — the napi layer maps these to a
/// JS error so the caller can surface guidance instead of a silent empty set.
pub fn search(
    content: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<Vec<StructuralMatch>, String> {
    let entry = languages::find_entry(ext)
        .ok_or_else(|| format!("structural search does not support .{ext} files"))?;
    let lang = AgLanguage {
        ts: entry.language.clone(),
        expando: expando_for_ext(ext),
    };

    let grep = lang.ast_grep(content);
    let root = grep.root();
    let mut out = Vec::new();

    match (pattern, rule) {
        (Some(p), None) => {
            if p.trim().is_empty() {
                return Err("pattern must not be empty".to_string());
            }
            let pat = Pattern::try_new(p, lang.clone())
                .map_err(|e| format!("invalid structural pattern: {e}"))?;
            for m in root.find_all(&pat) {
                out.push(to_match(&m));
            }
        }
        (None, Some(r)) => {
            if r.trim().is_empty() {
                return Err("rule must not be empty".to_string());
            }
            let env = DeserializeEnv::new(lang.clone());
            let serialized: SerializableRuleCore =
                from_str(r).map_err(|e| format!("invalid rule YAML: {e}"))?;
            let matcher = serialized
                .get_matcher(env)
                .map_err(|e| format!("invalid rule: {e}"))?;
            for m in root.find_all(&matcher) {
                out.push(to_match(&m));
            }
        }
        (Some(_), Some(_)) => {
            return Err("provide either `pattern` or `rule`, not both".to_string());
        }
        (None, None) => {
            return Err("structural search requires `pattern` or `rule`".to_string());
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
        search(src, ext, Some(pattern), None).expect("pattern search should succeed")
    }

    #[test]
    fn finds_call_and_captures_single_metavar() {
        let src = "const a = foo(bar);\nconst b = foo(baz);\n";
        let matches = run_pattern(src, "ts", "foo($X)");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start_line, 1);
        assert_eq!(matches[0].metavars.get("X").map(Vec::as_slice), Some(&["bar".to_string()][..]));
        assert_eq!(matches[1].metavars.get("X").map(Vec::as_slice), Some(&["baz".to_string()][..]));
    }

    #[test]
    fn captures_multi_metavar_as_list() {
        let src = "log(1, 2, 3);\n";
        let matches = run_pattern(src, "js", "log($$$ARGS)");
        assert_eq!(matches.len(), 1);
        // ast-grep's `$$$` captures EVERY node in the list, separators
        // included — matching the CLI exactly (KPI #2). We deliberately do not
        // strip the commas.
        assert_eq!(
            matches[0].metavars.get("ARGS").map(Vec::as_slice),
            Some(&["1".to_string(), ",".to_string(), "2".to_string(), ",".to_string(), "3".to_string()][..])
        );
    }

    #[test]
    fn comment_and_string_immunity() {
        // KPI #1: a literal `eval(x)` inside a comment and inside a string must
        // NOT match — only the real call site (line 3) does.
        let src = "// eval(evil)\nconst s = \"eval(evil)\";\neval(real);\n";
        let matches = run_pattern(src, "js", "eval($X)");
        assert_eq!(matches.len(), 1, "only the real call site matches");
        assert_eq!(matches[0].start_line, 3);
        assert_eq!(matches[0].metavars.get("X").map(Vec::as_slice), Some(&["real".to_string()][..]));
    }

    #[test]
    fn python_pattern_with_expando_char() {
        // Python's expando char is µ, not $ — exercises pre_process_pattern.
        let src = "print(hello)\nprint(world)\n";
        let matches = run_pattern(src, "py", "print($X)");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].metavars.get("X").map(Vec::as_slice), Some(&["hello".to_string()][..]));
    }

    #[test]
    fn rust_pattern_with_expando_char() {
        let src = "fn main() {\n    println(a);\n    println(b);\n}\n";
        let matches = run_pattern(src, "rs", "println($X)");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn relational_rule_inside_function() {
        // KPI: a rule that plain patterns cannot express — `await` calls that
        // are `inside` a for-loop. `stopBy: end` walks all ancestors.
        let src = "async function f() {\n  for (const x of xs) {\n    await g(x);\n  }\n  await h();\n}\n";
        let rule = "rule:\n  pattern: await $C\n  inside:\n    kind: for_in_statement\n    stopBy: end\n";
        let matches = search(src, "ts", None, Some(rule)).expect("rule search should succeed");
        assert_eq!(matches.len(), 1, "only the await inside the for-loop matches");
        assert_eq!(matches[0].start_line, 3);
    }

    #[test]
    fn unsupported_extension_errors() {
        match search("x", "zzz", Some("foo()"), None) {
            Err(e) => assert!(e.contains("does not support")),
            Ok(_) => panic!("expected an unsupported-extension error"),
        }
    }

    #[test]
    fn both_or_neither_query_errors() {
        assert!(search("x", "ts", Some("a"), Some("b")).is_err());
        assert!(search("x", "ts", None, None).is_err());
    }

    #[test]
    fn invalid_pattern_errors() {
        assert!(search("x", "ts", Some("   "), None).is_err());
    }
}
