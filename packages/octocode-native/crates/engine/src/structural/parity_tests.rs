//! Differential corpus: astSearch (Octo matcher) against astRewrite
//! (embedded ast-grep) over the same source and the same rule. Shared cases
//! must select the same spans with the same captures; every intended
//! difference is pinned here so a change on either side is noticed.
use super::rewrite::rewrite;
use super::search;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// 0-based start/end line and UTF-16 column, matched text, captures.
type Span = (u32, u32, u32, u32, String, BTreeMap<String, Vec<String>>);

/// ast-grep records the node a relational rule matched under this name; it
/// is not a user capture.
const AST_GREP_RELATIONAL_CAPTURE: &str = "secondary";

fn searched(ext: &str, source: &str, rule: &Value, extra: &Value) -> Result<Vec<Span>, String> {
    let found = if extra.is_null()
        && let Some(pattern) = rule
            .as_object()
            .filter(|o| o.len() == 1)
            .and_then(|o| o.get("pattern"))
            .and_then(Value::as_str)
    {
        search(source, ext, Some(pattern), None)
    } else {
        let mut document = json!({"rule": rule});
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                document[key] = value.clone();
            }
        }
        // JSON is valid YAML, so the same rule document feeds both engines.
        search(source, ext, None, Some(&document.to_string()))
    }?;
    Ok(found
        .into_iter()
        .map(|m| {
            (
                m.start_line - 1,
                m.start_col,
                m.end_line - 1,
                m.end_col,
                m.text,
                m.metavars.into_iter().collect(),
            )
        })
        .collect())
}

fn rewritten(
    language: &str,
    source: &str,
    rule: &Value,
    extra: &Value,
) -> Result<Vec<Span>, String> {
    let mut config = json!({"id": "parity", "language": language, "rule": rule, "fix": "X"});
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            config[key] = value.clone();
        }
    }
    Ok(rewrite(source, config)?
        .into_iter()
        .map(|m| {
            (
                m.range.start.line,
                m.range.start.column,
                m.range.end.line,
                m.range.end.column,
                m.text,
                m.captures
                    .into_iter()
                    .filter(|(name, _)| name != AST_GREP_RELATIONAL_CAPTURE)
                    .map(|(name, capture)| (name, capture.texts))
                    .collect(),
            )
        })
        .collect())
}

enum Expect {
    /// Same non-empty selection, spans and captures.
    Same,
    /// Same (possibly empty) selection, e.g. a relational miss.
    SameMaybeEmpty,
    /// Both engines reject the query.
    BothReject,
    /// astSearch rejects a rule feature only astRewrite supports.
    SearchRejects(&'static str),
    /// A documented difference: selection counts on each side.
    Differ { search: usize, rewrite: usize },
}

struct Case {
    name: &'static str,
    ext: &'static str,
    language: &'static str,
    source: &'static str,
    rule: Value,
    extra: Value,
    expect: Expect,
}

fn case(
    name: &'static str,
    ext: &'static str,
    language: &'static str,
    source: &'static str,
    rule: Value,
    expect: Expect,
) -> Case {
    Case {
        name,
        ext,
        language,
        source,
        rule,
        extra: Value::Null,
        expect,
    }
}

#[test]
fn search_and_rewrite_agree_on_the_shared_subset_and_pin_differences() {
    use Expect::*;
    let ts = |name, source, rule, expect| case(name, "ts", "typescript", source, rule, expect);
    let mut cases = vec![
        ts(
            "single metavar",
            "foo(a);\nfoo(b, c);\nbar(foo(d));\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        ts(
            "multi metavar keeps separators",
            "foo();\nfoo(a);\nfoo(a, b, c);\n",
            json!({"pattern": "foo($$$ARGS)"}),
            Same,
        ),
        ts(
            "repeated capture must agree",
            "a == a;\na == b;\nf(x) == f(x);\n",
            json!({"pattern": "$A == $A"}),
            Same,
        ),
        ts(
            "empty multi between punctuation",
            "f(1, 2, 3);\nf(1, 3);\nf(1);\n",
            json!({"pattern": "f(1, $$$MID, 3)"}),
            Same,
        ),
        ts(
            "empty multi before a literal",
            "foo(x);\nfoo(a, x);\nfoo(a, b);\n",
            json!({"pattern": "foo($$$A, x)"}),
            Same,
        ),
        ts(
            "ignored multi",
            "foo(1, 2);\nfoo();\n",
            json!({"pattern": "foo($$$)"}),
            Same,
        ),
        ts(
            "ignored single",
            "foo(1);\nfoo(1, 2);\n",
            json!({"pattern": "foo($_)"}),
            Same,
        ),
        ts(
            "block body capture",
            "function f() { a(); b(); }\nfunction g() {}\n",
            json!({"pattern": "function $F() { $$$BODY }"}),
            Same,
        ),
        ts(
            "method chain",
            "a.b().c();\nx.c();\n",
            json!({"pattern": "$O.c()"}),
            Same,
        ),
        ts(
            "trailing comma is candidate trivia",
            "foo(a,);\nfoo(a);\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        ts(
            "comments are candidate trivia",
            "foo(/* c */ a);\nfoo(a /* c */);\nfoo(\n  // c\n  a,\n);\n",
            json!({"pattern": "foo(a)"}),
            Same,
        ),
        ts(
            "partial syntax elsewhere in the file",
            "foo(a);\nconst = ;\nfoo(b);\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        ts(
            "utf-16 columns",
            "const é = foo(\"✓\");\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        ts(
            "inside default stops at the parent",
            "function g() { if (x) { foo(1); } }\nfoo(2);\n",
            json!({"pattern": "foo($X)", "inside": {"kind": "function_declaration"}}),
            SameMaybeEmpty,
        ),
        ts(
            "inside stopBy end",
            "function g() { if (x) { foo(1); } }\nfoo(2);\n",
            json!({"pattern": "foo($X)", "inside": {"kind": "function_declaration", "stopBy": "end"}}),
            Same,
        ),
        ts(
            "inside a pattern",
            "if (x) { foo(1); }\nfoo(2);\n",
            json!({"pattern": "foo($X)", "inside": {"pattern": "if ($C) { $$$B }", "stopBy": "end"}}),
            Same,
        ),
        ts(
            "has default checks children only",
            "function g() { foo(1); }\nfunction h() { if (x) { foo(2); } }\n",
            json!({"kind": "function_declaration", "has": {"pattern": "foo($X)"}}),
            SameMaybeEmpty,
        ),
        ts(
            "has stopBy end",
            "function g() { foo(1); }\nfunction h() { if (x) { foo(2); } }\n",
            json!({"kind": "function_declaration", "has": {"pattern": "foo($X)", "stopBy": "end"}}),
            Same,
        ),
        ts(
            "not",
            "foo(1);\nfoo(a);\n",
            json!({"all": [{"pattern": "foo($X)"}, {"not": {"pattern": "foo(1)"}}]}),
            Same,
        ),
        ts(
            "any",
            "foo(1);\nbar(2);\nbaz(3);\n",
            json!({"any": [{"pattern": "foo($X)"}, {"pattern": "bar($X)"}]}),
            Same,
        ),
        ts(
            "kind and regex",
            "const fooBar = 1;\nconst baz = 2;\n",
            json!({"kind": "identifier", "regex": "^foo"}),
            Same,
        ),
        ts(
            "kind only",
            "const a = 1;\nlet b = 2;\n",
            json!({"kind": "lexical_declaration"}),
            Same,
        ),
        case(
            "python call",
            "py",
            "python",
            "foo(1)\nfoo(a, b)\nx = foo(c)\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        case(
            "python trailing body capture",
            "py",
            "python",
            "def f(a):\n    return a\n\ndef g():\n    pass\n",
            json!({"pattern": "def $F($$$P):\n    $$$B"}),
            Same,
        ),
        case(
            "rust method",
            "rs",
            "rust",
            "fn m() { let a = x.unwrap(); y.unwrap(); z.expect(\"e\"); }\n",
            json!({"pattern": "$E.unwrap()"}),
            Same,
        ),
        case(
            "rust macro",
            "rs",
            "rust",
            "fn m() { println!(\"a\"); println!(\"{}\", b); }\n",
            json!({"pattern": "println!($$$A)"}),
            Same,
        ),
        case(
            "go statement",
            "go",
            "go",
            "package m\nfunc f() { if err != nil { return err }; if x != nil { return nil } }\n",
            json!({"pattern": "if err != nil { $$$B }"}),
            Same,
        ),
        case(
            "java call",
            "java",
            "java",
            "class A { void m() { foo(1); this.foo(2); } }\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        case(
            "tsx element",
            "tsx",
            "tsx",
            "const a = <Btn onClick={f} />;\nconst b = <Btn />;\n",
            json!({"pattern": "<Btn onClick={$H} />"}),
            Same,
        ),
        case(
            "c return",
            "c",
            "c",
            "int main() { return foo(1); }\n",
            json!({"pattern": "return $X;"}),
            Same,
        ),
        case(
            "cpp call",
            "cpp",
            "cpp",
            "int main() { foo(1); std::foo(2); }\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        case(
            "c# call",
            "cs",
            "cs",
            "class A { void M() { Log(1); Log(1, 2); } }\n",
            json!({"pattern": "Log($X)"}),
            Same,
        ),
        case(
            "scala call",
            "scala",
            "scala",
            "object A { foo(1); bar(foo(2)) }\n",
            json!({"pattern": "foo($X)"}),
            Same,
        ),
        ts(
            "invalid pattern",
            "foo(a);\n",
            json!({"pattern": "foo($X"}),
            BothReject,
        ),
        ts(
            "several top-level nodes",
            "function f() {\n  a();\n  b();\n}\n",
            json!({"pattern": "a(); b()"}),
            BothReject,
        ),
        case(
            "unsupported language",
            "kt",
            "kt",
            "fun m() { foo(1) }\n",
            json!({"pattern": "foo($X)"}),
            BothReject,
        ),
        ts(
            "follows is rewrite-only",
            "a();\nb();\n",
            json!({"pattern": "b()", "follows": {"pattern": "a()"}}),
            SearchRejects("unknown field `follows`"),
        ),
        // astSearch drops a statement's `;` and matches the expression
        // anywhere; ast-grep keeps the statement and its `;`.
        ts(
            "statement pattern",
            "foo(a);\nconst x = foo(b);\n",
            json!({"pattern": "foo($X);"}),
            Differ {
                search: 2,
                rewrite: 1,
            },
        ),
        // astSearch's `$K: $V` shorthand selects object pairs; ast-grep
        // parses the same text as a labeled statement.
        ts(
            "pair shorthand",
            "const o = { a: 1, b: f(2) };\n",
            json!({"pattern": "$K: $V"}),
            Differ {
                search: 2,
                rewrite: 0,
            },
        ),
        // ast-grep parses a bare C call pattern as a declaration; astSearch
        // supplies statement context.
        case(
            "bare c call",
            "c",
            "c",
            "int main() { foo(1); bar(foo(2)); return 0; }\n",
            json!({"pattern": "foo($X)"}),
            Differ {
                search: 2,
                rewrite: 0,
            },
        ),
    ];
    cases.push(Case {
        extra: json!({"constraints": {"X": {"kind": "number"}}}),
        ..ts(
            "constraints are rewrite-only",
            "foo(1);\nfoo(a);\n",
            json!({"pattern": "foo($X)"}),
            SearchRejects("unknown field `constraints`"),
        )
    });
    let mut failures = Vec::new();
    for case in &cases {
        let search = searched(case.ext, case.source, &case.rule, &case.extra);
        let rewrite = rewritten(case.language, case.source, &case.rule, &case.extra);
        let ok = match (&case.expect, &search, &rewrite) {
            (Same, Ok(s), Ok(r)) => !s.is_empty() && s == r,
            (SameMaybeEmpty, Ok(s), Ok(r)) => s == r,
            (BothReject, Err(_), Err(_)) => true,
            (SearchRejects(message), Err(error), Ok(_)) => error.contains(message),
            (
                Differ {
                    search: n,
                    rewrite: m,
                },
                Ok(s),
                Ok(r),
            ) => s.len() == *n && r.len() == *m,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}:\n  search={search:?}\n  rewrite={rewrite:?}",
                case.name
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// An unsupported multi-node pattern is an actionable error, not an empty
/// result that reads as absence.
#[test]
fn several_top_level_nodes_are_rejected_with_guidance() {
    let Err(error) = search("a();\nb();\n", "ts", Some("a(); b()"), None) else {
        panic!("a multi-node pattern must be rejected");
    };
    assert!(error.contains("2 top-level nodes"), "{error}");
    assert!(error.contains("`has`/`inside`"), "{error}");
}
