//! Cheap per-file signals behind `sort: "relevance"`, the key after the match
//! count and before the path (see `ripgrep_search::compare_recs`):
//!
//! 1. [`is_demoted_path`] — test, fixture, generated, bundled, and vendored
//!    paths rank after source paths.
//! 2. [`line_weight`] summed over a file's matched lines — a hit on the name a
//!    declaration introduces outranks the same token in ordinary code, which
//!    outranks it in a comment or a string literal.
//!
//! Both are lexical, per-line, and allocation-free: no parser, no index.

/// Weight of a matched line whose first match starts at byte `first_match`.
const DECLARATION_WEIGHT: u32 = 2;
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
    b"mut",
];

/// Whether the byte `first_match` falls on the name a declaration introduces:
/// `pub fn name`, `export default class Name`, `let mut name`, `#define NAME`.
fn declares_at(line: &[u8], indent: usize, first_match: usize) -> bool {
    let mut at = indent;
    let mut seen_keyword = false;
    for _ in 0..6 {
        let word_end = at
            + line[at..]
                .iter()
                .take_while(|byte| !byte.is_ascii_whitespace())
                .count();
        if word_end == at {
            return false;
        }
        let word = &line[at..word_end];
        if seen_keyword && word != b"mut" {
            return (at..word_end).contains(&first_match);
        }
        // `pub(crate)` and `pub(super)` are the `pub` modifier.
        let bare = word.split(|byte| *byte == b'(').next().unwrap_or(word);
        if DECLARATION_KEYWORDS.contains(&word) {
            seen_keyword = true;
        } else if !DECLARATION_MODIFIERS.contains(&bare) {
            return false;
        }
        at = word_end
            + line[word_end..]
                .iter()
                .take_while(|byte| byte.is_ascii_whitespace())
                .count();
    }
    false
}

/// Directory names whose files rank after source files.
const DEMOTED_DIRECTORIES: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "spec",
    "specs",
    "testdata",
    "fixtures",
    "__fixtures__",
    "__mocks__",
    "generated",
    "__generated__",
    "dist",
    "build",
    "vendor",
    "third_party",
];

/// File-name fragments that mark test, generated, or minified files.
const DEMOTED_FILE_MARKERS: &[&str] = &[
    "_test.",
    ".test.",
    "_spec.",
    ".spec.",
    ".generated.",
    "_generated.",
    ".pb.",
    "_pb2.",
    ".min.",
];

/// Whether a root-relative path is a test, fixture, generated, bundled, or
/// vendored file. Only the path below the search root is judged, so searching
/// inside `tests/` itself does not demote every result.
pub(crate) fn is_demoted_path(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase().replace('\\', "/");
    let mut parts = lower.rsplit('/');
    let name = parts.next().unwrap_or_default();
    parts.any(|dir| DEMOTED_DIRECTORIES.contains(&dir))
        || name.starts_with("test_")
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
