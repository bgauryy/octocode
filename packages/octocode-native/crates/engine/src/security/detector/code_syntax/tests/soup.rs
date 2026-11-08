//! The line scan's invariant (B165): wherever a grammar's tree places a
//! comment, the scan reads the line from a place at or before it
//! ([`comment_start`] after [`block_after`]), so no comment's text is left
//! unread on a scan's guess. A scan may read more than the tree (code after
//! a quote it cannot pair); it may never read less.
//!
//! Checked on generated token soup for each grammar, and, opt-in, on every
//! file of a corpus: `OCTOCODE_LEXER_CORPUS=<dir>` (and
//! `OCTOCODE_LEXER_CASES=<n>` for the soup's size).

use super::*;

/// Where a comment of the tree holds a byte the scan leaves unread, as
/// `(line number, byte offset)`, the first of each line.
fn unread_comment_bytes(text: &str, tree: &Tree, grammar: &LanguageEntry) -> Vec<(usize, usize)> {
    let mut comments = Vec::new();
    let mut cursor = tree.root_node().walk();
    'walk: loop {
        let node = cursor.node();
        if node.kind().contains("comment") && node.start_byte() < node.end_byte() {
            comments.push(node.byte_range());
        } else if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    let mut unread = Vec::new();
    let mut block = BlockState::default();
    let mut line_start = 0;
    let mut next_comment = 0;
    for (number, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches(['\n', '\r']);
        let line_end = line_start + body.len();
        let read_from = line_start.saturating_add(comment_start(body, block.is_open(), grammar));
        block = block_after(body, block, grammar);
        while comments
            .get(next_comment)
            .is_some_and(|comment| comment.end <= line_start)
        {
            next_comment += 1;
        }
        let first = comments[next_comment..]
            .iter()
            .take_while(|comment| comment.start < line_end)
            .map(|comment| comment.start.max(line_start))
            .find(|&at| at < line_end && at < read_from);
        if let Some(at) = first {
            unread.push((number + 1, at));
        }
        line_start += line.len();
    }
    unread
}

fn parse(text: &str, grammar: &LanguageEntry) -> Option<Tree> {
    crate::signatures::extractor::parse_until_stopped(text, &grammar.language, &|| false)
        .ok()
        .filter(|tree| !tree.root_node().has_error())
}

/// A small xorshift generator: the soup is the same on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(bound).expect("a small bound")).expect("in range")
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// The text pieces a literal or a comment is made of: every mark a scan
/// reads, quotes, escapes and line ends.
const PIECES: [&str; 20] = [
    "\"",
    "'",
    "`",
    "\\",
    "/*",
    "*/",
    "//",
    "#",
    "/",
    "*",
    "r\"",
    "r#\"",
    "\"#",
    "\"\"\"",
    "${",
    "}",
    "x",
    " ",
    "\n",
    "password: S",
];

fn filler(rng: &mut Rng, keep: impl Fn(&str) -> bool) -> String {
    let mut text = String::new();
    for _ in 0..rng.below(8) {
        let piece = rng.pick(&PIECES);
        let mut next = text.clone();
        next.push_str(piece);
        if keep(&next) {
            text = next;
        }
    }
    text
}

/// A double-quoted literal with escapes; `newlines` when the grammar lets
/// one run over a line end.
fn quoted(rng: &mut Rng, quote: char, newlines: bool) -> String {
    let body: String = filler(rng, |_| true)
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".to_owned(),
            '\n' if newlines => "\\\n".to_owned(),
            '\n' => "\\n".to_owned(),
            c if c == quote => format!("\\{c}"),
            c => c.to_string(),
        })
        .collect();
    format!("{quote}{body}{quote}")
}

fn line_comment(rng: &mut Rng, opener: &str) -> String {
    let body = filler(rng, |text| !text.contains('\n'));
    format!("{opener}{body}\n")
}

/// A block comment, holding nested ones when the grammar nests them.
fn block_comment(rng: &mut Rng, nests: bool, depth: usize) -> String {
    let mut body = filler(rng, |text| {
        !text.contains("*/") && !text.contains("/*") && !text.ends_with('/') && !text.ends_with('*')
    });
    if nests && depth < 3 && rng.below(3) == 0 {
        body.push_str(&block_comment(rng, nests, depth + 1));
        body.push(' ');
    }
    format!("/*{body}*/")
}

/// A raw literal closed by `close`, holding none.
fn raw(rng: &mut Rng, open: &str, close: &str) -> String {
    let mut body = filler(rng, |text| !text.contains(close));
    while format!("{body}{close}").find(close) != Some(body.len()) {
        body.pop();
    }
    format!("{open}{body}{close}")
}

/// One expression or item of a grammar's soup.
fn rust_piece(rng: &mut Rng) -> String {
    match rng.below(9) {
        0 => format!("let _ = {};", quoted(rng, '"', true)),
        1 => format!("let _ = {};", raw(rng, "r#\"", "\"#")),
        2 => format!(
            "let _ = ({},{});",
            rng.pick(&[
                "'\\x41'",
                "'\\u{41}'",
                "'\\''",
                "'\\\\'",
                "'\"'",
                "'/'",
                "b'\\x22'",
                "'a'"
            ]),
            rng.pick(&["'\"'", "'*'", "'/'", "'\\n'"])
        ),
        // Lifetimes and labels: a `'` that opens no char literal.
        3 => rng
            .pick(&[
                "let _: &'static str = \"\";",
                "fn g<'a, 'b: 'a>(x: &'a str, _: &'b [u8]) -> &'a str { x }",
                "struct S<'a> { t: Option<&'a str>, c: char }",
                "fn h<'a>(_: &'a char) -> char { '\"' }",
                "impl<'a> S<'a> { fn t(&self) -> &'a str { \"'\" } }",
                "'outer: loop { break 'outer; }",
                "let _ = |x: &'_ str| x.len();",
            ])
            .to_owned(),
        4 => {
            let opener = rng.pick(&["//", "///", "//!"]);
            line_comment(rng, opener)
        }
        5 => block_comment(rng, true, 0),
        6 => format!("let _ = b{};", quoted(rng, '"', false)),
        7 => "#[allow(unused)] let _ = 1;".to_owned(),
        _ => "let _ = 1 / 2 * 3;".to_owned(),
    }
}

fn script_piece(rng: &mut Rng) -> String {
    match rng.below(9) {
        0 => format!("x = {};", quoted(rng, '"', false)),
        1 => format!("x = {};", quoted(rng, '\'', false)),
        2 => {
            let body = filler(rng, |_| true)
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace("${", "$\\{");
            format!("x = `{body}${{a}}`;")
        }
        3 => format!(
            "x = {}.test(c);",
            rng.pick(&[
                "/r\"/", "/\"/", "/a\"b/", "/'/", "/\\/\\*/", "/[*/]/", "/R#\"/"
            ])
        ),
        4 => line_comment(rng, "//"),
        5 => block_comment(rng, false, 0),
        6 => "x = a / b / c;".to_owned(),
        7 => "x = a-- > 0;".to_owned(),
        _ => "if (x) { y(); }".to_owned(),
    }
}

fn python_piece(rng: &mut Rng) -> String {
    match rng.below(6) {
        0 => format!("x = {}", quoted(rng, '"', false)),
        1 => format!("x = r{}", quoted(rng, '\'', false)),
        2 => format!("x = {}", raw(rng, "\"\"\"", "\"\"\"").replace('\\', "")),
        3 => line_comment(rng, "#"),
        4 => "x = a / b".to_owned(),
        _ => "x = '/*'".to_owned(),
    }
}

/// C, C++, CUDA, Java, C#, Go and Scala: strings, characters, comments of
/// both kinds, and a line comment a `\\` continues.
fn c_family_piece(rng: &mut Rng, nests: bool) -> String {
    match rng.below(7) {
        0 => format!("x = {};", quoted(rng, '"', false)),
        1 => format!(
            "x = {};",
            rng.pick(&["'\"'", "'\\''", "'\\\\'", "'/'", "'*'"])
        ),
        2 => line_comment(rng, "//"),
        3 => block_comment(rng, nests, 0),
        4 => format!(
            "// {} \\\n{}\n",
            filler(rng, |text| !text.contains('\n')),
            filler(rng, |text| !text.contains('\n'))
        ),
        5 => "x = a / b / c;".to_owned(),
        _ => "x = 1;".to_owned(),
    }
}

/// A soup of `grammar`'s pieces in the frame its grammar parses statements in.
fn soup(rng: &mut Rng, language: &str) -> String {
    let pieces = rng.below(10) + 1;
    let mut body = String::new();
    for _ in 0..pieces {
        let piece = match language {
            "rust" => rust_piece(rng),
            "python" => python_piece(rng),
            "javascript" | "typescript" => script_piece(rng),
            _ => c_family_piece(rng, language == "scala"),
        };
        body.push_str(&piece);
        body.push_str(rng.pick(&["\n", " "]));
    }
    match language {
        "rust" => format!("fn f() {{\n{body}\n}}\n"),
        "python" | "javascript" | "typescript" => format!("{body}\n"),
        "c" | "cpp" | "cuda" => format!("void f(void) {{\nint x, a, b, c;\n{body}\n}}\n"),
        "java" => format!("class A {{ void f() {{\nObject x; int a, b, c;\n{body}\n}} }}\n"),
        "csharp" => {
            format!("class A {{ void F() {{\nobject x; int a = 1, b = 1, c = 1;\n{body}\n}} }}\n")
        }
        "go" => format!(
            "package a\nfunc f() {{\nvar x interface{{}}; a, b, c := 1, 1, 1\n{body}\n_ = x; _ = a; _ = b; _ = c }}\n"
        ),
        _ => format!(
            "object A {{ def f() = {{\nvar x: Any = 0; val a = 1; val b = 1; val c = 1\n{body}\n}} }}\n"
        ),
    }
}

/// The extension of each grammar the soup is made for.
const SOUP_GRAMMARS: [(&str, &str); 11] = [
    ("rust", "rs"),
    ("typescript", "ts"),
    ("javascript", "js"),
    ("python", "py"),
    ("c", "c"),
    ("cpp", "cpp"),
    ("cuda", "cu"),
    ("java", "java"),
    ("csharp", "cs"),
    ("go", "go"),
    ("scala", "scala"),
];

#[test]
fn the_line_scan_reads_every_comment_byte_of_generated_soup() {
    let cases: usize = std::env::var("OCTOCODE_LEXER_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(2_000);
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for (language, extension) in SOUP_GRAMMARS {
        let grammar = grammar_of(&format!("a.{extension}")).expect("a grammar");
        let mut rng = Rng(language.bytes().fold(0x9E37_79B9_7F4A_7C15, |seed, byte| {
            seed.rotate_left(5) ^ u64::from(byte)
        }));
        let mut parsed = 0;
        for _ in 0..cases {
            let text = soup(&mut rng, language);
            let Some(tree) = parse(&text, grammar) else {
                continue;
            };
            parsed += 1;
            if let Some((line, at)) = unread_comment_bytes(&text, &tree, grammar).first() {
                failures.push(format!("{language} line {line} byte {at}: {text:?}"));
            }
            if language == "rust" {
                failures.extend(
                    disagreements(&text, &tree)
                        .into_iter()
                        .map(|line| format!("rust {line} in {text:?}")),
                );
            }
        }
        report.push(format!("{language}: {parsed} of {cases} parsed"));
        assert!(
            parsed * 4 > cases,
            "{language}: only {parsed} of {cases} soups parse"
        );
    }
    eprintln!("{}", report.join("\n"));
    failures.sort_by_key(String::len);
    assert!(
        failures.is_empty(),
        "{} soups read less than their tree, shortest first:\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every source file under `dir` with a grammar.
fn grammar_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            grammar_files(&path, out);
        } else if path.to_str().and_then(grammar_of).is_some() {
            out.push(path);
        }
    }
}

/// The invariant over a corpus of real files, one row per grammar: files,
/// bytes, files whose tree is read, and the files a scan reads less than.
#[test]
#[ignore = "reads the corpus OCTOCODE_LEXER_CORPUS names"]
fn the_line_scan_reads_every_comment_byte_of_a_corpus() {
    let corpus =
        std::env::var("OCTOCODE_LEXER_CORPUS").expect("OCTOCODE_LEXER_CORPUS names a directory");
    let mut files = Vec::new();
    grammar_files(std::path::Path::new(&corpus), &mut files);
    let mut rows: std::collections::BTreeMap<String, (usize, usize, usize, Vec<String>)> =
        Default::default();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let name = file.to_string_lossy().into_owned();
        let grammar = grammar_of(&name).expect("a grammar file");
        let row = rows
            .entry(grammar.language_id.unwrap_or("?").to_owned())
            .or_default();
        row.0 += 1;
        row.1 += text.len();
        let Some(tree) = parse(&text, grammar) else {
            continue;
        };
        row.2 += 1;
        if let Some((line, at)) = unread_comment_bytes(&text, &tree, grammar).first() {
            let line_text = text.lines().nth(line - 1).unwrap_or_default();
            row.3
                .push(format!("{name}:{line} (byte {at}): {line_text:?}"));
        }
    }
    let mut unread = 0;
    for (language, (files, bytes, parsed, misses)) in &rows {
        eprintln!(
            "{language}: {files} files, {bytes} bytes, {parsed} parsed, {} unread",
            misses.len()
        );
        for miss in misses.iter().take(3) {
            eprintln!("  {miss}");
        }
        unread += misses.len();
    }
    assert_eq!(unread, 0, "files whose scan reads less than their tree");
}
