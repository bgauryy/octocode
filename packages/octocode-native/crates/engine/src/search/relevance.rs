//! Cheap per-file signals behind `sort: "relevance"` (see
//! `ripgrep_search::compare_recs`). For a bare-identifier search, a source
//! file with a [`DECLARATION_WEIGHT`] hit ranks before the match count; the
//! rest order files after the count and before the path:
//!
//! 1. [`is_demoted_path`] — test, fixture, generated, bundled, and vendored
//!    paths rank after source paths.
//! 2. [`line_weight`] summed over a file's matched lines — a hit on the name a
//!    declaration introduces outranks the same token in ordinary code, which
//!    outranks it in a comment or a string literal.
//!
//! Both are lexical, per-line, and allocation-free: no parser, no index.

use crate::text::test_paths::is_test_path;

/// Weight of a matched line whose first match starts at byte `first_match`.
pub(crate) const DECLARATION_WEIGHT: u32 = 2;
const CODE_WEIGHT: u32 = 1;
const PROSE_WEIGHT: u32 = 0;

/// Relevance weight of one matched line: [`DECLARATION_WEIGHT`] when the first
/// match is the name a declaration introduces, `0` inside a comment or a
/// string literal, `1` for any other code.
pub(crate) fn line_weight(line: &[u8], first_match: usize) -> u32 {
    let first_match = first_match.min(line.len());
    let indent = line
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    if starts_comment(&line[indent..]) || in_string_or_comment(line, indent, first_match) {
        return PROSE_WEIGHT;
    }
    if declares_at(line, indent, first_match) {
        DECLARATION_WEIGHT
    } else {
        CODE_WEIGHT
    }
}

/// Rank of one matched line among a file's hits, used to choose which rows a
/// clipped file shows first: 3 when the match is the name a declaration
/// introduces, 2 on a deciding statement (an assignment, a branch, or a
/// `return`/`raise`/`throw`), 1 on other code, 0 in a comment or string.
pub(crate) fn line_rank(line: &[u8], first_match: usize) -> u32 {
    match line_weight(line, first_match) {
        PROSE_WEIGHT => 0,
        DECLARATION_WEIGHT => 3,
        _ => {
            let indent = line
                .iter()
                .take_while(|byte| byte.is_ascii_whitespace())
                .count();
            if opens_with_decision(&line[indent..]) || assigns(&line[indent..]) {
                2
            } else {
                1
            }
        }
    }
}

/// Words that open a statement deciding control flow or a result.
const DECISION_KEYWORDS: &[&[u8]] = &[
    b"if", b"elif", b"else", b"case", b"when", b"switch", b"match", b"while", b"for", b"return",
    b"raise", b"throw", b"yield", b"guard", b"unless", b"except", b"catch",
];

/// First word after any closing braces (`} else if`) is a decision keyword.
fn opens_with_decision(text: &[u8]) -> bool {
    let start = text
        .iter()
        .take_while(|byte| matches!(byte, b'}' | b')') || byte.is_ascii_whitespace())
        .count();
    let word = text[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
        .count();
    DECISION_KEYWORDS.contains(&&text[start..start + word])
}

/// An assignment operator outside brackets, strings, and a trailing `//`
/// comment: `x = y`, `x := y`, `x += y`. Comparisons (`==`, `!=`, `<=`, `>=`),
/// arrows (`=>`), and keyword arguments inside a call (`f(a=1)`) do not count.
fn assigns(text: &[u8]) -> bool {
    let mut depth = 0usize;
    let mut quote = None;
    let mut at = 0;
    while at < text.len() {
        let byte = text[at];
        match quote {
            Some(_) if byte == b'\\' => at += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None => match byte {
                b'"' | b'\'' | b'`' => quote = Some(byte),
                b'/' if text.get(at + 1) == Some(&b'/') => return false,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b'=' if depth == 0 => {
                    let before = at.checked_sub(1).map(|i| text[i]);
                    let after = text.get(at + 1).copied();
                    let comparison = matches!(after, Some(b'=' | b'>'))
                        || matches!(before, Some(b'=' | b'!' | b'<' | b'>'));
                    if !comparison {
                        return true;
                    }
                }
                _ => {}
            },
        }
        at += 1;
    }
    false
}

fn starts_comment(text: &[u8]) -> bool {
    let next_is_blank = |at: usize| text.get(at).is_none_or(u8::is_ascii_whitespace);
    text.starts_with(b"//")
        || text.starts_with(b"/*")
        || text.starts_with(b"<!--")
        || (text.starts_with(b"*") && (next_is_blank(1) || text.get(1) == Some(&b'/')))
        || (text.starts_with(b"#") && (next_is_blank(1) || text.get(1) == Some(&b'#')))
        || (text.starts_with(b"--") && next_is_blank(2))
}

/// Whether byte `end` sits inside a `"`/`` ` `` string or after a `//` or
/// `/*` comment opener on this line. Single quotes are not tracked: they are
/// Rust lifetimes and char literals as often as strings.
fn in_string_or_comment(line: &[u8], start: usize, end: usize) -> bool {
    let mut quote = None;
    let mut at = start;
    while at < end {
        let byte = line[at];
        match quote {
            Some(_) if byte == b'\\' => at += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'`' => quote = Some(byte),
            None if byte == b'/' && matches!(line.get(at + 1), Some(b'/' | b'*')) => return true,
            None => {}
        }
        at += 1;
    }
    quote.is_some()
}

/// Words that introduce a declared name.
const DECLARATION_KEYWORDS: &[&[u8]] = &[
    b"fn",
    b"def",
    b"class",
    b"struct",
    b"enum",
    b"trait",
    b"impl",
    b"interface",
    b"type",
    b"typedef",
    b"const",
    b"let",
    b"var",
    b"val",
    b"func",
    b"function",
    b"module",
    b"mod",
    b"namespace",
    b"union",
    b"record",
    b"object",
    b"macro_rules!",
    b"#define",
];

/// Words that may precede a declaration keyword, or its name (`let mut x`).
const DECLARATION_MODIFIERS: &[&[u8]] = &[
    b"pub",
    b"export",
    b"default",
    b"public",
    b"private",
    b"protected",
    b"internal",
    b"static",
    b"async",
    b"abstract",
    b"final",
    b"override",
    b"extern",
    b"unsafe",
    b"inline",
    b"virtual",
    b"open",
    b"sealed",
    b"data",
    b"declare",
    b"readonly",
    b"synchronized",
    b"native",
    b"transient",
    b"volatile",
    b"mut",
];

/// Whether the byte `first_match` falls on the name a declaration introduces:
/// `pub fn name`, `export default class Name`, `let mut name`, `#define NAME`,
/// a Go method `func (r *T) name`, or a C-family member led by modifiers and a
/// type (`public static <T> T name(`, `private final long name =`).
fn declares_at(line: &[u8], indent: usize, first_match: usize) -> bool {
    let words = Words { line, at: indent };
    let mut lead = Lead::Start;
    // `<K, V>` type parameters and `(r *T)` receivers span several words.
    let mut group: Option<u8> = None;
    for (start, end) in words.take(10) {
        let word = &line[start..end];
        if let Some(close) = group {
            if word.ends_with(&[close]) {
                group = None;
            }
            continue;
        }
        match lead {
            Lead::Keyword { .. } if word == b"mut" => {}
            Lead::Keyword { func: true } if word.starts_with(b"(") => {
                group = (!word.ends_with(b")")).then_some(b')');
            }
            Lead::Keyword { .. } => return (start..end).contains(&first_match),
            Lead::Typed => return (start..start + ident_len(word)).contains(&first_match),
            Lead::Start | Lead::Modified => {
                // `pub(crate)` and `pub(super)` are the `pub` modifier.
                let bare = word.split(|byte| *byte == b'(').next().unwrap_or(word);
                if DECLARATION_KEYWORDS.contains(&word) {
                    lead = Lead::Keyword {
                        func: word == b"func",
                    };
                } else if DECLARATION_MODIFIERS.contains(&bare) || is_annotation(word) {
                    lead = Lead::Modified;
                } else if lead == Lead::Start {
                    return false;
                } else if word.starts_with(b"<") {
                    group = (!word.ends_with(b">")).then_some(b'>');
                } else if ident_len(word) == 0 || STATEMENT_WORDS.contains(&bare) {
                    return false;
                } else if names_here(line, start, word) {
                    return (start..start + ident_len(word)).contains(&first_match);
                } else {
                    lead = Lead::Typed;
                }
            }
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lead {
    /// No word yet.
    Start,
    /// Only modifiers and annotations so far: a type or a name follows.
    Modified,
    /// A declaration keyword: the next word is the name (after a Go
    /// method's receiver when the keyword is `func`).
    Keyword { func: bool },
    /// Modifiers and a type: the next word is the name.
    Typed,
}

/// Words that open a statement rather than name a type after modifiers
/// (`pub use x;`, `export default new X()`).
const STATEMENT_WORDS: &[&[u8]] = &[
    b"use",
    b"import",
    b"return",
    b"new",
    b"await",
    b"throw",
    b"yield",
    b"extends",
    b"implements",
    b"in",
    b"of",
    b"as",
    b"is",
];

/// `@Override`, `@Nullable`, `@Component(...)`.
fn is_annotation(word: &[u8]) -> bool {
    word.len() > 1 && word[0] == b'@' && (word[1].is_ascii_alphabetic() || word[1] == b'_')
}

/// Length of the identifier prefix of `word` (`name` in `name(`, `name;`).
fn ident_len(word: &[u8]) -> usize {
    if word.first().is_none_or(u8::is_ascii_digit) {
        return 0;
    }
    word.iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        .count()
}

/// Whether the identifier opening `word` (at `start`) is itself the declared
/// name rather than a type: a parameter list, initializer, type annotation, or
/// terminator follows it (`name(`, `name =`, `name:`, `name;`, `name,`).
fn names_here(line: &[u8], start: usize, word: &[u8]) -> bool {
    let after = start + ident_len(word);
    let next = line[after..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|offset| after + offset);
    match next.map(|at| (line[at], line.get(at + 1).copied())) {
        Some((b'=', Some(b'=' | b'>'))) => false,
        Some((b'(' | b'=' | b':' | b';' | b',', _)) => true,
        _ => false,
    }
}

/// Whitespace-separated `(start, end)` word spans from `at`.
struct Words<'a> {
    line: &'a [u8],
    at: usize,
}

impl Iterator for Words<'_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let line = self.line;
        let start = self.at
            + line[self.at..]
                .iter()
                .take_while(|byte| byte.is_ascii_whitespace())
                .count();
        let end = start
            + line[start..]
                .iter()
                .take_while(|byte| !byte.is_ascii_whitespace())
                .count();
        self.at = end;
        (end > start).then_some((start, end))
    }
}

/// Directory names of generated, bundled, and vendored files, which rank
/// after source files (test directories are [`is_test_path`]'s).
const DEMOTED_DIRECTORIES: &[&str] = &[
    "generated",
    "__generated__",
    "dist",
    "build",
    "vendor",
    "third_party",
];

/// File-name fragments that mark generated or minified files.
const DEMOTED_FILE_MARKERS: &[&str] = &[".generated.", "_generated.", ".pb.", "_pb2.", ".min."];

/// Directory names of generated files, which rank after every hand-written
/// file whatever their match count.
const GENERATED_DIRECTORIES: &[&str] = &["generated", "__generated__"];

/// File-name fragments of generated files (a subset of
/// [`DEMOTED_FILE_MARKERS`]; minified bundles are only demoted).
const GENERATED_FILE_MARKERS: &[&str] = &[".generated.", "_generated.", ".pb.", "_pb2."];

/// Leading bytes read for a generated-file header.
pub(crate) const GENERATED_HEADER_BYTES: u64 = 512;

/// Header phrases of generated files (`Code generated by … DO NOT EDIT.`,
/// `@generated`, `Automatically generated by …, do not edit.`), compared
/// case-insensitively.
const GENERATED_HEADER_MARKERS: &[&str] = &[
    "do not edit",
    "@generated",
    "code generated",
    "automatically generated",
    "auto-generated",
    "autogenerated",
];

/// Whether a root-relative path names a generated file by its directory or
/// name. Only the path below the search root is judged.
pub(crate) fn is_generated_path(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase().replace('\\', "/");
    let mut parts = lower.rsplit('/');
    let name = parts.next().unwrap_or_default();
    parts.any(|dir| GENERATED_DIRECTORIES.contains(&dir))
        || GENERATED_FILE_MARKERS
            .iter()
            .any(|marker| name.contains(marker))
}

/// Whether a file's leading bytes declare it generated: a marker phrase in
/// its first lines.
pub(crate) fn has_generated_header(prefix: &[u8]) -> bool {
    let head = String::from_utf8_lossy(prefix).to_ascii_lowercase();
    head.lines()
        .take(5)
        .any(|line| GENERATED_HEADER_MARKERS.iter().any(|marker| line.contains(marker)))
}

/// Whether a root-relative path is a test, fixture, generated, bundled, or
/// vendored file. Only the path below the search root is judged, so searching
/// inside `tests/` itself does not demote every result.
pub(crate) fn is_demoted_path(relative: &str) -> bool {
    if is_test_path(relative) {
        return true;
    }
    let lower = relative.to_ascii_lowercase().replace('\\', "/");
    let mut parts = lower.rsplit('/');
    let name = parts.next().unwrap_or_default();
    parts.any(|dir| DEMOTED_DIRECTORIES.contains(&dir))
        || DEMOTED_FILE_MARKERS
            .iter()
            .any(|marker| name.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weight(line: &str, token: &str) -> u32 {
        line_weight(line.as_bytes(), line.find(token).expect("token in line"))
    }

    #[test]
    fn declaration_names_outrank_code_which_outranks_prose() {
        for line in [
            "pub fn parse_config(path: &str) {",
            "    pub(crate) async fn parse_config() {",
            "export default function parse_config() {",
            "def parse_config(path):",
            "class parse_config:",
            "let mut parse_config = 1;",
            "#define parse_config 1",
        ] {
            assert_eq!(weight(line, "parse_config"), DECLARATION_WEIGHT, "{line}");
        }
        for line in [
            "let cfg = parse_config(path);",
            "impl Parser for parse_config {",
            "parse_config();",
            "#[derive(parse_config)]",
            "fn load(x: &'a str) -> Config { parse_config(x) }",
        ] {
            assert_eq!(weight(line, "parse_config"), CODE_WEIGHT, "{line}");
        }
        for line in [
            "// parse_config reads the file",
            "    /// Calls parse_config.",
            " * parse_config is here",
            "# parse_config in python",
            "-- parse_config in sql",
            "let label = \"parse_config\";",
            "let label = \"a \\\" parse_config\";",
            "let x = 1; // parse_config",
            "const s = `${a} parse_config`;",
        ] {
            assert_eq!(weight(line, "parse_config"), PROSE_WEIGHT, "{line}");
        }
    }

    /// C-family declarations name no keyword: modifiers, a type, then the
    /// name (`public static <T> T firstNonNull(`, `private final long size =`).
    /// A Go method's receiver sits between `func` and its name.
    #[test]
    fn typed_member_and_receiver_declarations_are_declarations() {
        for (line, token) in [
            (
                "  public static <T> T firstNonNull(@Nullable T first, @Nullable T second) {",
                "firstNonNull",
            ),
            (
                "  private final long maximumSize = UNSET_INT;",
                "maximumSize",
            ),
            ("    @Override public String toString() {", "toString"),
            (
                "static int parse_config(const char *path) {",
                "parse_config",
            ),
            ("  public CacheBuilder(Ticker ticker) {", "CacheBuilder"),
            ("  private readonly scene = new Scene();", "scene"),
            (
                "func (ng *Engine) exec(ctx context.Context) error {",
                "exec",
            ),
            ("func NewEngine(opts EngineOpts) *Engine {", "NewEngine"),
        ] {
            assert_eq!(weight(line, token), DECLARATION_WEIGHT, "{line}");
        }
        for (line, token) in [
            ("    return firstNonNull(a, b);", "firstNonNull"),
            ("  public void run() { firstNonNull(x); }", "firstNonNull"),
            (
                "  public static <T> T firstNonNull(@Nullable T first) {",
                "first)",
            ),
            ("    this.maximumSize = maximumSize;", "maximumSize"),
            (
                "func (ng *Engine) exec(ctx context.Context) error {",
                "Engine",
            ),
            ("  static_assert(parse_config(x));", "parse_config"),
        ] {
            assert_ne!(weight(line, token), DECLARATION_WEIGHT, "{line}");
        }
    }

    fn rank(line: &str, token: &str) -> u32 {
        line_rank(line.as_bytes(), line.find(token).expect("token in line"))
    }

    #[test]
    fn hit_rank_puts_declarations_then_deciding_statements_before_plain_code_and_prose() {
        assert_eq!(rank("pub fn sample_limit() {", "sample_limit"), 3);
        for line in [
            "    this.maximumSize = maximumSize;",
            "\tz.SampleLimit = 0",
            "  long maximumSize = UNSET_INT;",
            "\tapp := appenderWithLimits(sl.sampleLimit)",
            "\tcase errors.Is(err, errSampleLimit):",
            "    } else if (maximumSize > 0) {",
            "        elif too_many_fields:",
            "\treturn 0, errSampleLimit",
            "            raise self.model.MultipleObjectsReturned(",
            "total += sample_limit;",
        ] {
            let token = [
                "maximumSize",
                "SampleLimit",
                "sampleLimit",
                "too_many_fields",
                "MultipleObjectsReturned",
                "sample_limit",
            ]
            .into_iter()
            .find(|token| line.contains(token))
            .expect("token");
            assert_eq!(rank(line, token), 2, "{line}");
        }
        for line in [
            "    checkArgument(maximumSize >= 0, \"negative\");",
            "        this.maximumSize == UNSET_INT,",
            "\tsl.metrics.targetScrapeSampleLimit.Inc()",
            "    builder.maximumSize(size, limit => limit + 1);",
            "    configure(maximumSize=10)",
            "\t\tsampleLimit: int(opts.limit),",
        ] {
            let token = ["maximumSize", "SampleLimit", "sampleLimit"]
                .into_iter()
                .find(|token| line.contains(token))
                .expect("token");
            assert_eq!(rank(line, token), 1, "{line}");
        }
        assert_eq!(rank("   * {@link #maximumSize(long)}", "maximumSize"), 0);
        assert_eq!(rank("x = 1 // maximumSize", "maximumSize"), 0);
    }

    #[test]
    fn generated_files_are_named_by_path_or_header() {
        for path in ["src/generated/api.rs", "api.generated.ts", "a/b_pb2.py", "x.pb.go"] {
            assert!(is_generated_path(path), "{path}");
        }
        for path in ["dist/index.js", "app.min.js", "tests/a.rs", "src/generator.rs"] {
            assert!(!is_generated_path(path), "{path}");
        }
        for head in [
            "/* Automatically generated by generate-command-code.py, do not edit. */\n",
            "// Code generated by protoc-gen-go. DO NOT EDIT.\n\npackage x\n",
            "# @generated by tool\n",
            "#!/usr/bin/env python\n# -*- coding: utf-8 -*-\n# This file is autogenerated\n",
        ] {
            assert!(has_generated_header(head.as_bytes()), "{head}");
        }
        for head in [
            "fn main() {}\n",
            "// Generates the config table.\nfn generate() {}\n",
            "a\nb\nc\nd\ne\n// DO NOT EDIT below line five\n",
        ] {
            assert!(!has_generated_header(head.as_bytes()), "{head}");
        }
    }

    #[test]
    fn test_generated_and_vendored_paths_are_demoted_below_the_root() {
        for path in [
            "tests/config.rs",
            "pkg/__tests__/a.ts",
            "config_test.go",
            "src/config.test.ts",
            "src/config.spec.js",
            "test_config.py",
            "src/generated/api.rs",
            "api.generated.ts",
            "dist/index.js",
            "vendor/lib/a.go",
            "app.min.js",
            "Tests\\Unit.cs",
        ] {
            assert!(is_demoted_path(path), "{path}");
        }
        for path in [
            "src/config.rs",
            "latest/config.rs",
            "contest.rs",
            "src/testing_utils.rs",
            "build.rs",
            "distance.rs",
        ] {
            assert!(!is_demoted_path(path), "{path}");
        }
    }
}
