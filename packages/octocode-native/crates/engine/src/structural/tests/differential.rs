//! The literal prefilter never drops a match: every query runs over a varied
//! fixture set with no prefilter and with the query's own, and the match
//! sets must be equal.
//!
//! The queries are every pattern and rule the structural test sources spell
//! (read from those sources when the test runs, so a new test's query joins
//! the corpus), the astSearch queries of the e2e suites
//! (`packages/octocode-benchmark/local/e2e`), and the RFC's measured patterns
//! (`.octocode/octocode-rfc-generator/2026-11-08-ast-search-misses`, M2).

use super::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One match, as a scan reports it.
type MatchKey = (String, u32, u32, u32, u32, String);

/// The grammar fixtures of the e2e grammar suite
/// (`e2e/harness/grammar-fixtures.mjs`), one per grammar.
const GRAMMAR_FIXTURES: [(&str, &str); 13] = [
    (
        "ts",
        "export const LIMIT = 3;\nexport interface Shape { area(): number }\nexport class Widget implements Shape {\n  area(): number { return helper(LIMIT); }\n}\nexport function helper(n: number): number { return n * 2; }\nexport function run(): number {\n  const s = \"😀\"; return helper(s.length);\n}\n",
    ),
    (
        "tsx",
        "export const LIMIT = 3;\nexport function helper(n: number) { return n * 2; }\nexport function Widget() {\n  const s = \"😀\"; return <div>{helper(s.length)}</div>;\n}\nexport function run() { return helper(LIMIT); }\n",
    ),
    (
        "js",
        "const LIMIT = 3;\nclass Widget {\n  area() { return helper(LIMIT); }\n}\nfunction helper(n) { return n * 2; }\nfunction run() {\n  const s = \"😀\"; return helper(s.length);\n}\nmodule.exports = { Widget, helper, run };\n",
    ),
    (
        "py",
        "LIMIT = 3\n\nclass Widget:\n    def area(self):\n        return helper(LIMIT)\n\ndef helper(n):\n    return n * 2\n\ndef run():\n    s = \"😀\"; return helper(len(s))\n",
    ),
    (
        "go",
        "package demo\n\nconst LIMIT = 3\n\ntype Widget struct{ n int }\n\nfunc (w Widget) Area() int { return helper(LIMIT) }\n\nfunc helper(n int) int { return n * 2 }\n\nfunc run() int {\n\ts := \"😀\"; return helper(len(s))\n}\n",
    ),
    (
        "rs",
        "pub const LIMIT: i32 = 3;\npub struct Widget { n: i32 }\nimpl Widget {\n    pub fn area(&self) -> i32 { helper(LIMIT + self.n) }\n}\npub fn helper(n: i32) -> i32 { n * 2 }\npub fn run() -> i32 {\n    let s = \"😀\"; helper(s.len() as i32)\n}\n",
    ),
    (
        "java",
        "public class Widget {\n  static final int LIMIT = 3;\n  int area() { return helper(LIMIT); }\n  static int helper(int n) { return n * 2; }\n  static int run() {\n    String s = \"😀\"; return helper(s.length());\n  }\n}\n",
    ),
    (
        "c",
        "#define LIMIT 3\nstruct Widget { int n; };\nint helper(int n) { return n * 2; }\nint run(void) {\n  const char *s = \"😀\"; return helper((int)s[0]);\n}\n",
    ),
    (
        "cpp",
        "const int LIMIT = 3;\nint helper(int n) { return n * 2; }\nclass Widget {\n public:\n  int area() { return helper(LIMIT); }\n};\nint run() {\n  const char *s = \"😀\"; return helper(s[0]);\n}\n",
    ),
    (
        "cu",
        "const int LIMIT = 3;\n__device__ int helper(int n) { return n * 2; }\n__global__ void run() {\n  const char *s = \"😀\"; int value = helper((int)s[0] + LIMIT);\n}\n",
    ),
    (
        "cs",
        "public class Widget {\n  const int LIMIT = 3;\n  int Area() { return Helper(LIMIT); }\n  static int Helper(int n) { return n * 2; }\n  static int Run() {\n    var s = \"😀\"; return Helper(s.Length);\n  }\n}\n",
    ),
    (
        "scala",
        "object Widget {\n  val LIMIT = 3\n  def helper(n: Int): Int = n * 2\n  def run(): Int = {\n    val s = \"😀\"; helper(s.length)\n  }\n}\n",
    ),
    (
        "asm",
        "section .text\nglobal run\nhelper:\n    mov eax, edi\n    ret\nrun:\n    call helper\n    ret\n",
    ),
];

/// Matches inside macro bodies (`octo::MacroBodies`): Rust token trees,
/// nested invocations and C-family `#define` bodies.
const MACRO_FIXTURES: [(&str, &str); 8] = [
    (
        "rs",
        "fn t() {\n    assert!(check(x));\n    assert_eq!(check(y).unwrap(), 2);\n    let v = vec![a.unwrap(); 2];\n    outer!(inner!(deep.unwrap()));\n    debug_log!(\"{}\", format!(\"{e}\"));\n    x.map_err(|e| format!(\"{e}\"));\n    check(z);\n}\n",
    ),
    (
        "rs",
        "macro_rules! wrap {\n    ($e:expr) => { check($e).unwrap() };\n}\nfn u() { wrap!(1); println!(\"{}\", helper(2)); }\n",
    ),
    (
        "c",
        "#define CHECK(x) check(x)\n#define FREE(p) zfree(p)\nint helper(int n) { return CHECK(n) + zfree(0); }\n",
    ),
    ("rs", RV_AST2_SOURCE),
    (
        "rs",
        "fn k() {\n    start();\n    assert!(check(z));\n    stop();\n}\nfn kk() { ok(); outer_fn(m!(zzz())); }\n",
    ),
    (
        "c",
        "#define CALL(x) foo(x)\n#define F(a) do { first(a); check(a); } while (0)\nvoid g(void) { foo(1); }\n",
    ),
    // `json!` objects: no Rust, so each macro call they spell is re-parsed on
    // its own (B214; the RV-C3eH round 3 probe's lines).
    (
        "rs",
        "pub fn build(name: &str) -> V {\n    json!({\n        \"greeting\": format!(\"hello {}\", name),\n        \"nested\": { \"inner\": format!(\"deep {}\", name) },\n        \"list\": vec![check(name)],\n    })\n}\n",
    ),
    (
        "rs",
        "fn f(v: &V, i: u32) {\n    let a = json!({\"name\":format!(\"f{i:03}.rs\"),\"type\":\"file\",\"size\":1});\n    let b = json!({\"kind\": \"token_tree\", \"has\": {\"pattern\": \"check($A)\"}});\n    let c = json!({\n        \"id\": v[\"id\"].to_string().trim_matches('\"'),\n    });\n}\n",
    ),
];

/// Rules over macro calls inside `json!` objects: every node kind the
/// object's recovered re-parse could misread, and relations into the calls.
fn macro_call_rules() -> [Value; 6] {
    [
        json!({"kind": "call_expression"}),
        json!({"kind": "macro_invocation"}),
        json!({"kind": "block"}),
        json!({"pattern": "format!($$$A)", "inside": {"kind": "function_item", "stopBy": "end"}}),
        json!({"kind": "macro_invocation", "has": {"pattern": "check($A)", "stopBy": "end"}}),
        json!({"kind": "string_literal", "has": {"kind": "escape_sequence"}}),
    ]
}

/// Rules whose relation reaches into a macro body from a node outside it
/// (`has`, and `precedes`, `follows` or `inside` through a `has`), so the
/// macro-body gate must re-parse a body by the `has` target's literals.
fn relation_into_body_rules() -> [Value; 9] {
    let checked =
        json!({"kind": "expression_statement", "has": {"pattern": "check($A)", "stopBy": "end"}});
    [
        json!({"kind": "function_item", "has": {"pattern": "check($A)", "stopBy": "end"}}),
        json!({"pattern": "outer_fn($$$)", "has": {"kind": "call_expression", "stopBy": "end"}}),
        json!({"kind": "token_tree", "has": {"pattern": "check($A)"}}),
        json!({"kind": "function_item", "regex": "^fn k", "not": {"has": {"pattern": "check($A)", "stopBy": "end"}}}),
        json!({"kind": "expression_statement", "precedes": checked.clone()}),
        json!({"kind": "expression_statement", "follows": checked}),
        json!({"pattern": "stop()", "inside": {"kind": "function_item", "has": {"pattern": "check($A)", "stopBy": "end"}, "stopBy": "end"}}),
        json!({"kind": "preproc_function_def", "has": {"pattern": "foo($A)", "stopBy": "end"}}),
        json!({"kind": "preproc_arg", "has": {"pattern": "foo($A)"}}),
    ]
}

/// The RV-AST2 review's source: calls inside `assert!` bodies, and one
/// outside.
const RV_AST2_SOURCE: &str = "fn t(){ assert!(check(x)); assert!(verify(w)); check(y); }\n";

/// The RV-AST2 review's rules: an `any` branch whose narrowest group is the
/// `inside` literal, which sits outside the macro body holding the match.
fn rv_ast2_rules() -> [Value; 2] {
    [
        json!({"any": [{"all": [
            {"any": [{"pattern": "check($A)"}, {"pattern": "verify($A)"}]},
            {"inside": {"pattern": "assert!($$$)", "stopBy": "end"}}
        ]}]}),
        json!({"any": [{
            "inside": {"pattern": "assert!($$$)", "stopBy": "end"},
            "any": [{"pattern": "check($A)"}, {"pattern": "verify($A)"}]
        }]}),
    ]
}

/// The RFC's M2 patterns (files the prefilter admits on the native crates).
const M2_PATTERNS: [&str; 5] = [
    "serde_json::from_str($X)",
    "Error::new(ErrorCode::$C, $M)",
    "$X.map_err(|$E| format!($$$))",
    "std::fs::read_to_string($P)",
    "tracing::debug!($$$)",
];

/// The queries the e2e suites send astSearch, and conjunctions that hold
/// literals in several places.
fn fixed_queries() -> Vec<(Option<String>, Option<Value>)> {
    let mut queries: Vec<(Option<String>, Option<Value>)> = [
        "add($A, $B)",
        "add(8, 9)",
        "greet($A)",
        "greet:",
        "probe($A)",
        "helper($A)",
        "Helper($A)",
        "helper(s.len() as i32)",
        "$X.unwrap()",
        "$X.unwrap($Y)",
        "return helper($A)",
        "fn run() -> i32 { $$$ }",
        "check($A)",
        "zfree($P)",
        "assert!($$$)",
    ]
    .into_iter()
    .chain(M2_PATTERNS)
    .map(|pattern| (Some(pattern.to_owned()), None))
    .collect();
    let rules = [
        json!({"all": [{"pattern": "add($A, $B)"}, {"inside": {"kind": "return_statement", "stopBy": "end"}}]}),
        json!({"kind": "function_item", "has": {"pattern": "$X.unwrap()", "stopBy": "end"}}),
        json!({"all": [{"pattern": "helper($A)"}, {"inside": {"pattern": "fn run() -> i32 { $$$ }", "stopBy": "end"}}]}),
        json!({"all": [{"any": [{"pattern": "helper($A)"}, {"pattern": "len($A)"}]}, {"inside": {"kind": "return_statement", "stopBy": "end"}}]}),
        json!({"all": [{"pattern": "$X.map_err($F)"}, {"has": {"pattern": "format!($$$)", "stopBy": "end"}}]}),
        json!({"kind": "call_expression", "has": {"kind": "identifier", "regex": "^helper$"}}),
        json!({"any": [{"pattern": "helper($A)"}, {"pattern": "Helper($A)"}]}),
        json!({"pattern": "helper($A)", "not": {"inside": {"pattern": "fn area(&self) -> i32 { $$$ }", "stopBy": "end"}}}),
        json!({"kind": "call_expression", "inside": {"pattern": "assert!($$$)", "stopBy": "end"}}),
        json!({"kind": "call_expression", "inside": {"pattern": "assert_eq!($$$)", "stopBy": "end"}}),
        json!({"pattern": "$X.unwrap()", "inside": {"pattern": "outer!($$$)", "stopBy": "end"}}),
        json!({"pattern": "check($A)", "inside": {"kind": "macro_invocation", "stopBy": "end"}}),
        json!({"kind": "call_expression", "inside": {"kind": "preproc_function_def", "stopBy": "end"}}),
        json!({"pattern": "format!($$$)", "inside": {"pattern": "debug_log!($$$)", "stopBy": "end"}}),
    ];
    queries.extend(
        rules
            .into_iter()
            .chain(rv_ast2_rules())
            .chain(relation_into_body_rules())
            .chain(macro_call_rules())
            .map(|rule| (None, Some(rule))),
    );
    for kind in [
        "function_declaration",
        "function_definition",
        "function_item",
        "method_declaration",
        "label",
    ] {
        queries.push((None, Some(json!({"kind": kind}))));
    }
    queries
}

fn engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every Rust source under `dir`, recursively, in path order.
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The structural test sources: each `tests.rs` and each file under a
/// `tests` folder.
fn structural_test_sources() -> Vec<PathBuf> {
    rust_sources(&engine_dir().join("src/structural"))
        .into_iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| name == "tests.rs")
                || path.components().any(|part| part.as_os_str() == "tests")
        })
        .collect()
}

/// The end of the string, raw string or char literal at `at` (a `"`, an
/// `r`/`r#` raw start, or a `'`), and the string's value; `None` when no
/// literal starts there (a lifetime, an identifier ending in `r`).
fn literal_at(src: &str, at: usize) -> Option<(usize, Option<String>)> {
    let rest = &src[at..];
    if let Some(raw) = rest.strip_prefix('r') {
        let starts_word = src[..at]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch == '_' || ch.is_alphanumeric()));
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        if !starts_word || !raw[hashes..].starts_with('"') {
            return None;
        }
        let body = at + 1 + hashes + 1;
        let close = format!("\"{}", "#".repeat(hashes));
        let end = src[body..].find(&close)? + body;
        return Some((end + close.len(), Some(src[body..end].to_owned())));
    }
    if let Some(quoted) = rest.strip_prefix('\'') {
        let mut chars = quoted.chars();
        let first = chars.next()?;
        if first == '\\' {
            let close = quoted[1..].find('\'')? + 2;
            return Some((at + 1 + close, None));
        }
        return (chars.next() == Some('\'')).then(|| (at + 2 + first.len_utf8(), None));
    }
    let mut value = String::new();
    let mut chars = rest.char_indices().skip(1);
    while let Some((offset, ch)) = chars.next() {
        match ch {
            '"' => return Some((at + offset + 1, Some(value))),
            '\\' => match chars.next()?.1 {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                '0' => value.push('\0'),
                'u' => {
                    let mut hex = String::new();
                    for (_, ch) in chars.by_ref() {
                        match ch {
                            '{' => {}
                            '}' => break,
                            _ => hex.push(ch),
                        }
                    }
                    value.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                '\n' => {
                    while chars.clone().next().is_some_and(|(_, c)| c.is_whitespace()) {
                        chars.next();
                    }
                }
                other => value.push(other),
            },
            _ => value.push(ch),
        }
    }
    None
}

/// The string literals of `src`, and the text inside each `json!( … )`.
fn literals_and_json(src: &str) -> (Vec<String>, Vec<String>) {
    let (mut strings, mut json) = (Vec::new(), Vec::new());
    let mut opened = Vec::new();
    let mut at = 0;
    while at < src.len() {
        let rest = &src[at..];
        if rest.starts_with("//") {
            at += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("json!(") {
            opened.push((at + "json!(".len(), 0usize));
            at += "json!(".len();
            continue;
        }
        let ch = rest.chars().next().unwrap_or_default();
        if matches!(ch, '"' | '\'' | 'r')
            && let Some((end, value)) = literal_at(src, at)
        {
            strings.extend(value);
            at = end;
            continue;
        }
        match ch {
            '(' | '[' | '{' => {
                if let Some((_, depth)) = opened.last_mut() {
                    *depth += 1;
                }
            }
            ')' | ']' | '}' => match opened.last_mut() {
                Some((start, 0)) if ch == ')' => {
                    json.push(src[*start..at].to_owned());
                    opened.pop();
                }
                Some((_, depth)) => *depth = depth.saturating_sub(1),
                None => {}
            },
            _ => {}
        }
        at += ch.len_utf8();
    }
    (strings, json)
}

/// Every pattern and rule the structural test sources spell: each string
/// literal one line long that could be a pattern (it holds a metavariable
/// or a call), and each `json!` object that decodes as JSON.
fn suite_queries() -> Vec<(Option<String>, Option<Value>)> {
    let mut patterns = BTreeSet::new();
    let mut rules = BTreeSet::new();
    for path in structural_test_sources() {
        let src = fs::read_to_string(&path).expect("read test source");
        let (strings, json) = literals_and_json(&src);
        patterns.extend(strings.into_iter().filter(|text| {
            !text.trim().is_empty()
                && !text.trim_end().contains('\n')
                && (text.contains('$') || text.contains('('))
        }));
        rules.extend(
            json.iter()
                .filter_map(|text| serde_json::from_str::<Value>(text).ok())
                .filter(Value::is_object)
                .map(|rule| rule.to_string()),
        );
    }
    patterns
        .into_iter()
        .map(|pattern| (Some(pattern), None))
        .chain(rules.into_iter().map(|rule| {
            (
                None,
                Some(serde_json::from_str(&rule).expect("a rule decoded once")),
            )
        }))
        .collect()
}

fn options(
    root: &Path,
    pattern: Option<String>,
    rule: Option<Value>,
) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern,
        rule,
        include: None,
        exclude: None,
        default_exclude_dir: None,
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(u32::MAX),
        skip_files: None,
        max_file_bytes: None,
    }
}

/// Every match of a scan, by file and span.
fn match_keys(result: &StructuralSearchFilesDetailedResult) -> BTreeSet<MatchKey> {
    result
        .files
        .iter()
        .flat_map(|file| {
            file.matches.held().iter().map(|found| {
                (
                    file.path.clone(),
                    found.start_line,
                    found.start_col,
                    found.end_line,
                    found.end_col,
                    found.text.clone(),
                )
            })
        })
        .collect()
}

/// The corpus roots: the e2e grammar fixtures (written to a temporary
/// folder), the e2e suites' fixture folder and the structural tests.
fn fixture_roots(grammars: &TmpDir) -> Vec<PathBuf> {
    write_fixtures(grammars);
    let e2e = engine_dir().join("../../../octocode-benchmark/local/e2e/fixtures");
    assert!(e2e.is_dir(), "the e2e fixtures folder: {}", e2e.display());
    vec![
        grammars.0.clone(),
        e2e,
        engine_dir().join("src/structural/tests"),
        engine_dir().join("src/structural/octo/tests"),
        engine_dir().join("src/structural/query"),
    ]
}

/// Writes the grammar and macro fixtures under `dir`.
fn write_fixtures(dir: &TmpDir) {
    for (index, (ext, src)) in GRAMMAR_FIXTURES.iter().chain(&MACRO_FIXTURES).enumerate() {
        dir.write(&format!("fixture{index}.{ext}"), src);
    }
}

/// What one query's two scans of one root showed.
#[derive(Default)]
struct Compared {
    scans: usize,
    matched: usize,
    skipped: u64,
    differences: Vec<String>,
}

impl Compared {
    /// Scans `root` with the query's prefilter and with none, and records
    /// any match one finds and the other does not.
    fn query(&mut self, pattern: Option<&String>, rule: Option<&Value>, root: &Path) {
        let filtered = search_files_detailed(options(root, pattern.cloned(), rule.cloned()));
        let unfiltered = search_files_unfiltered(options(root, pattern.cloned(), rule.cloned()));
        let (filtered, unfiltered) = match (filtered, unfiltered) {
            (Ok(filtered), Ok(unfiltered)) => (filtered, unfiltered),
            (Err(_), Err(_)) => return,
            (filtered, unfiltered) => {
                self.differences.push(format!(
                    "{pattern:?} {rule:?} under {}: one run failed ({:?} / {:?})",
                    root.display(),
                    filtered.err(),
                    unfiltered.err()
                ));
                return;
            }
        };
        self.scans += 1;
        self.skipped += u64::from(filtered.skipped_by_pre_filter);
        let (with, without) = (match_keys(&filtered), match_keys(&unfiltered));
        self.matched += without.len();
        if with != without {
            self.differences.push(format!(
                "{pattern:?} {rule:?} under {}: {} missing, {} extra",
                root.display(),
                without.difference(&with).count(),
                with.difference(&without).count()
            ));
        }
    }

    fn add(&mut self, other: Self) {
        self.scans += other.scans;
        self.matched += other.matched;
        self.skipped += other.skipped;
        self.differences.extend(other.differences);
    }
}

/// Every query over every root, with the literal gates (the prefilter, the
/// macro-body gate) and with none. Queries run on plain threads, one share
/// each: the unfiltered switch is per thread, and a rayon worker could steal
/// another query's scan while it is set.
fn compare_all(queries: &[(Option<String>, Option<Value>)], roots: &[PathBuf]) -> Compared {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut total = Compared::default();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|share| {
                scope.spawn(move || {
                    let mut compared = Compared::default();
                    for (pattern, rule) in queries.iter().skip(share).step_by(threads) {
                        for root in roots {
                            compared.query(pattern.as_ref(), rule.as_ref(), root);
                        }
                    }
                    compared
                })
            })
            .collect();
        for worker in workers {
            total.add(worker.join().expect("a differential worker"));
        }
    });
    eprintln!(
        "prefilter differential: {} queries, {} scans, {} matches, {} files skipped",
        queries.len(),
        total.scans,
        total.matched,
        total.skipped
    );
    assert!(total.differences.is_empty(), "{:#?}", total.differences);
    assert!(
        total.matched > 0 && total.skipped > 0,
        "{} matches, {} skipped",
        total.matched,
        total.skipped
    );
    total
}

/// The default-run differential: the fixed queries (e2e, M2, conjunctions,
/// macro rules) over the grammar and macro fixtures.
#[test]
fn the_literal_gates_keep_every_match_of_the_fixed_queries() {
    let fixtures = TmpDir::new();
    write_fixtures(&fixtures);
    compare_all(&fixed_queries(), std::slice::from_ref(&fixtures.0));
}

/// The RV-AST2 rules find inside `assert!` bodies what they find with no
/// literal gate: an `any` branch's `inside` literal must not decide whether
/// a body is re-parsed.
#[test]
fn an_any_branch_inside_a_macro_keeps_its_matches() {
    let fixtures = TmpDir::new();
    fixtures.write("rv.rs", RV_AST2_SOURCE);
    for rule in rv_ast2_rules() {
        let options = || options(&fixtures.0, None, Some(rule.clone()));
        let reference = match_keys(&search_files_unfiltered(options()).expect("unfiltered"));
        let gated = match_keys(&search_files_detailed(options()).expect("gated"));
        assert!(!reference.is_empty(), "{rule}");
        assert_eq!(gated, reference, "{rule}");
    }
}

/// The full differential: every pattern and rule of the structural test
/// sources, plus the fixed queries, over every root (about a minute in a
/// debug build, so it runs in the heavy tier).
#[test]
#[ignore = "full corpus; run: cargo test -p octocode-engine --lib -- --ignored structural::tests::differential::the_prefilter_keeps_every_match_of_the_corpus"]
fn the_prefilter_keeps_every_match_of_the_corpus() {
    let grammars = TmpDir::new();
    let roots = fixture_roots(&grammars);
    let queries: Vec<_> = suite_queries().into_iter().chain(fixed_queries()).collect();
    assert!(queries.len() >= 200, "corpus of {} queries", queries.len());
    compare_all(&queries, &roots);
}

/// The native crates' Rust sources under the extent audit (every match from a
/// macro body lies inside its body and agrees with its bracket groups, or
/// the test binary aborts): the macro-call rules, with the counts and the
/// macro bodies disclosed. Run on demand:
/// `-- --ignored macro_body_matches_over_the_native_crates --nocapture`.
#[test]
#[ignore = "scans the native crates; run on demand"]
fn macro_body_matches_over_the_native_crates() {
    let crates = engine_dir().join("..");
    let rules = std::iter::once(json!({"pattern": "format!($$$A)"})).chain(macro_call_rules());
    for rule in rules {
        let mut options = options(&crates, None, Some(rule.clone()));
        options.include = Some(vec!["**/*.rs".to_owned()]);
        let scan = search_files_detailed(options).expect("scan the crates");
        let bodies: usize = scan.files.iter().map(|file| file.recovered_bodies.len()).sum();
        eprintln!(
            "crates {rule}: {} matches, {bodies} recovered bodies named",
            scan.total_matches
        );
    }
}

/// RFC M2: the files each measured pattern parses over the native crates'
/// Rust sources, with the prefilter and with none (the match sets must be
/// equal). Run on demand: `-- --ignored m2_files_parsed --nocapture`.
#[test]
#[ignore = "measures the native crates; run on demand"]
fn m2_files_parsed_over_the_native_crates() {
    let crates = engine_dir().join("..");
    for pattern in M2_PATTERNS {
        let scan = |unfiltered: bool| {
            let mut options = options(&crates, Some(pattern.to_owned()), None);
            options.include = Some(vec!["**/*.rs".to_owned()]);
            if unfiltered {
                search_files_unfiltered(options)
            } else {
                search_files_detailed(options)
            }
            .expect("scan the crates")
        };
        let (filtered, unfiltered) = (scan(false), scan(true));
        eprintln!(
            "M2 {pattern}: parsed {} skippedByPreFilter {} of {} (preFilter {}, anchor {:?}), matches {}",
            filtered.parsed_files,
            filtered.skipped_by_pre_filter,
            unfiltered.parsed_files,
            filtered.query.pre_filter,
            filtered.query.literal_anchor,
            filtered.total_matches
        );
        assert_eq!(match_keys(&filtered), match_keys(&unfiltered), "{pattern}");
    }
}
