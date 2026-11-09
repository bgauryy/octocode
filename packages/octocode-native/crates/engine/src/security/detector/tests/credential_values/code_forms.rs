//! Credential literals and comments in source code (B165): the forms a
//! credential takes beyond `name = "literal"`, and the comments that carry one.
//! Every value is fake.

use super::config_entries::{hides, keeps};
use super::*;

/// Each of `cases` read as the file at its path, in a read and in storage.
fn hidden_in_files(cases: &[(&str, &str)], secrets: &[&str]) {
    for (path, content) in cases {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, secrets);
        }
    }
}

fn kept_in_files(cases: &[(&str, &str)]) {
    for (path, content) in cases {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            keeps(content, scope);
        }
    }
}

/// A literal that backs an environment read is the credential, whichever name
/// the declaration, the variable or neither carries before the operator.
#[test]
fn a_fallback_literal_of_an_environment_read_is_redacted() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "const password = process.env.DB_PASS || \"Fake0Moo9Zed\";\n",
            ),
            (
                "a.ts",
                "const dbAuth = process.env.DB_PASS || \"Fake0Moo9Zed\";\n",
            ),
            (
                "a.ts",
                "const x = process.env[\"DB_PASS\"] ?? 'Fake0Moo9Zed';\n",
            ),
            (
                "a.py",
                "password = os.environ.get(\"DB_PASS\", \"Fake0Moo9Zed\")\n",
            ),
            (
                "a.py",
                "password = os.environ.get(\"DB_PASS\") or \"Fake0Moo9Zed\"\n",
            ),
            (
                "a.py",
                "db_auth = os.getenv(\"DB_PASS\", \"Fake0Moo9Zed\")\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    kept_in_files(&[
        (
            "a.ts",
            "const host = process.env.HOST || \"localhost:8080\";\n",
        ),
        ("a.ts", "const password = process.env.DB_PASS || \"\";\n"),
        ("a.py", "port = os.environ.get(\"PORT\", \"8080\")\n"),
        ("a.py", "path = os.environ.get(\"PWD\", \"/home/app\")\n"),
    ]);
}

#[test]
fn rust_raw_literals_are_redacted() {
    hidden_in_files(
        &[
            ("a.rs", "const PASSWORD: &str = r#\"Fake0Moo9Zed\"#;\n"),
            ("a.rs", "let password = r\"Fake0Moo9Zed\";\n"),
            ("a.rs", "let password = b\"Fake0Moo9Zed\";\n"),
        ],
        &["Fake0Moo9Zed"],
    );
}

/// A literal split in pieces is judged as the value it joins to.
#[test]
fn split_literals_are_redacted_as_their_joined_value() {
    let hidden = hides(
        "const password = \"Fake0\" + \"Moo9Zed\";\n",
        Scope::File("a.ts"),
        &["Fake0", "Moo9Zed"],
    );
    assert!(hidden.starts_with("const password = \""), "{hidden}");
    hidden_in_files(
        &[
            ("a.ts", "const password = 'Fake0' + 'Moo9' + 'Zed';\n"),
            ("a.py", "password = \"Fake0\" + \"Moo9Zed\"\n"),
        ],
        &["Fake0", "Moo9"],
    );
    kept_in_files(&[
        ("a.ts", "const label = \"Enter\" + \"your name\";\n"),
        ("a.ts", "const password = \"a\" + name;\n"),
    ]);
}

/// Pieces of one literal may use any quote type, and Python joins literals
/// that only sit side by side (`"a" "b"`).
#[test]
fn mixed_quote_and_adjacent_pieces_are_redacted_as_their_joined_value() {
    hidden_in_files(
        &[
            ("a.ts", "const password = \"Fake0\" + 'Moo9' + `Zed`;\n"),
            ("a.ts", "const password = 'Fake0' + \"Moo9Zed\";\n"),
            ("a.py", "password = \"Fake0\" \"Moo9Zed\"\n"),
            ("a.py", "password = 'Fake0' \"Moo9\" 'Zed'\n"),
        ],
        &["Fake0", "Moo9"],
    );
    let hidden = hides(
        "password = 'Fake0' \"Moo9Zed\"\n",
        Scope::File("a.py"),
        &["Fake0", "Moo9Zed"],
    );
    assert!(hidden.starts_with("password = '"), "{hidden}");
    assert!(hidden.trim_end().ends_with('"'), "{hidden}");
    kept_in_files(&[
        ("a.py", "label = 'Enter' \"your name\"\n"),
        ("a.ts", "const label = 'Enter' + `your name`;\n"),
    ]);
}

/// A comment that carries a credential is redacted wherever it sits on its
/// line, and whatever words lead the key: the syntax tree places the comment,
/// not the line's first characters.
#[test]
fn credentials_in_trailing_inline_and_prose_comments_are_redacted() {
    hidden_in_files(
        &[
            ("a.ts", "const x = 1; // password: Fake0Moo9Zed\n"),
            ("a.ts", "const x = 1; // DB_PASSWORD=Fake0Moo9Zed\n"),
            ("a.ts", "const x = 1; /* password: Fake0Moo9Zed */\n"),
            ("a.ts", "/* password: Fake0Moo9Zed */ const x = 1;\n"),
            ("a.ts", "// set the password: Fake0Moo9Zed\n"),
            ("a.py", "x = 1  # password=Fake0Moo9Zed\n"),
            ("a.rs", "let x = 1; // api_token: Fake0Moo9Zed\n"),
            ("a.go", "package a\n\nvar x = 1 // password: Fake0Moo9Zed\n"),
        ],
        &["Fake0Moo9Zed"],
    );
    // Prose after a bare `NAME=value` is the value's own only where the file
    // class says so (a stored text may be any file).
    hides(
        "// use DB_PASSWORD=Fake0Moo9Zed here\n",
        Scope::Stored(Some("a.ts")),
        &["Fake0Moo9Zed"],
    );
    let inline = hides(
        "const x = 1; /* password: Fake0Moo9Zed */\n",
        Scope::File("a.ts"),
        &["Fake0Moo9Zed"],
    );
    assert!(
        inline.ends_with("*/\n"),
        "the comment still closes: {inline}"
    );
}

/// Code and comments that only talk about a credential stay readable.
#[test]
fn code_and_prose_beside_comments_stay_readable() {
    kept_in_files(&[
        ("a.ts", "f.evidence.passed = true; // password: required\n"),
        ("a.ts", "const x = 1; // the password is required\n"),
        ("a.ts", "const x = 1; // the password: required\n"),
        ("a.ts", "f.evidence.passed = true;\n"),
        ("a.py", "x = 1  # the token: optional\n"),
        (
            "a.rs",
            "/// `key` as one reference token: `~` as `~0`, `/` as `~1`.\n",
        ),
        (
            "a.rs",
            "/// The store key: the analysis, the text's digest and the path (each\n",
        ),
        (
            "a.ts",
            " * Parse dotenv text into a { KEY: VALUE } map. Strict KEY=VALUE, `#` comments,\n",
        ),
        (
            "a.rs",
            "//! the result goes to stderr as `# key: value` notes, so stdout pipes\n",
        ),
    ]);
}

/// A returned line of source is a fragment that does not parse, so the syntax
/// cannot place it in code. What it holds still says: a struct field whose
/// value is a type expression is no credential, in any scope.
#[test]
fn a_type_expression_under_a_credential_name_is_readable() {
    for content in [
        "    tokens: Arc<Mutex<Tokens>>,\n",
        "    secret: Option<Vec<String>>,\n",
        "    password: HashMap<String, Value>,\n",
    ] {
        for scope in [
            Scope::File("a.rs"),
            Scope::Leaf("a.rs"),
            Scope::Stored(Some("a.rs")),
            Scope::Pathless,
        ] {
            keeps(content, scope);
        }
    }
    hidden_in_files(
        &[
            ("a.yml", "password: Fake0<Moo9Zed>\n"),
            ("a.yml", "password: Fake0Moo9Zed<\n"),
        ],
        &["Moo9Zed"],
    );
}

/// A text parses for its comments only when a comment may carry a credential:
/// a credential word assigned after a comment opener, or inside a block
/// comment. Code that merely assigns a credential-named field never parses.
#[test]
fn only_a_text_with_a_credential_in_a_comment_needs_its_comments_read() {
    use crate::security::detector::code_syntax::{grammar_of, has_credential_assignment};
    let rust = grammar_of("a.rs").expect("a Rust grammar");
    for text in [
        "x = 1; // password: Fake0Moo9Zed\n",
        "# token = Fake0Moo9Zed\n",
        "#[derive(Debug)] // token = Fake0Moo9Zed\n",
        "  # password: Fake0Moo9Zed\n",
        "select 1; -- password = Fake0Moo9Zed\n",
        "/*\n password: Fake0Moo9Zed\n*/\n",
    ] {
        assert!(has_credential_assignment(text, rust), "{text}");
    }
    for text in [
        "pub input_tokens: i64,\n",
        "let token = make(); // fine\n",
        "fn f() {}\n",
        "let glob = \"**/*.rs\";\npub input_tokens: i64,\n",
        "/* a note */\npub input_tokens: i64,\n",
        "#[doc = \"keywords: a page keeps only rows holding every term\"]\n",
        "/// A `next` key, whatever tool's row it sits on: every enum has it.\n",
    ] {
        assert!(!has_credential_assignment(text, rust), "{text}");
    }
}

/// Comment marks inside strings, raw strings and character literals do not
/// hide the real comment that follows, and a string or comment that never
/// ends is still read: the gate that skips the parse only ever errs toward it.
#[test]
fn comment_marks_in_literals_do_not_hide_a_real_comment_credential() {
    hidden_in_files(
        &[
            ("a.rs", "let a = \"/* x\"; // password: Fake0Moo9Zed\n"),
            (
                "a.rs",
                "let a = r#\"/* \"q\" // \"#; // password: Fake0Moo9Zed\n",
            ),
            (
                "a.rs",
                "let c = '\"'; /*\n password: Fake0Moo9Zed\n*/\nlet x = 1;\n",
            ),
            (
                "a.rs",
                "let c = '/'; let d = \"//\"; // password: Fake0Moo9Zed\n",
            ),
            ("a.ts", "const a = \"/*\"; // DB_PASSWORD=Fake0Moo9Zed\n"),
            ("a.rs", "let a = \"abc; // password: Fake0Moo9Zed\n"),
            ("a.rs", "/* password: Fake0Moo9Zed\nfn f() {}\n"),
        ],
        &["Fake0Moo9Zed"],
    );
}

/// A comment the lexical gate would not see is still read: `#[` and `#!`
/// open a comment wherever `#` does, and only a Rust file reads them as an
/// attribute (`.py` stands for a grammar whose `#` is a comment).
#[test]
fn hash_comments_that_start_like_attributes_are_read() {
    hidden_in_files(
        &[
            ("a.py", "#[TODO] password: Fake0Moo9Zed\n"),
            ("a.py", "#! password: Fake0Moo9Zed\n"),
            ("a.py", "x = 1 #[x] password: Fake0Moo9Zed\n"),
        ],
        &["Fake0Moo9Zed"],
    );
}

/// A double quote that nothing closes, in code, in a regular expression or in
/// a single-quoted string, hides no block comment that opens after it.
#[test]
fn a_stray_quote_hides_no_block_comment() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "if (/\"/.test(x)) { /* open\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.ts",
                "const s = 'He said \"hi'; /* open\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.ts",
                "f(/\"/, /* open \"quoted\" word\nsee the password: Fake0Moo9Zed */\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
}

/// A line comment that holds a mark opening a block comment is read two ways:
/// tree-sitter-rust opens a block at `/////*`, rustc reads a line comment. The
/// scan takes the mark as an opener in every grammar, and the tree decides.
#[test]
fn a_line_comment_holding_a_block_opener_hides_no_block_comment() {
    hidden_in_files(
        &[
            (
                "a.rs",
                "fn f() {\n/////* open\nsee the password: Fake0Moo9Zed */\n}\n",
            ),
            (
                "a.rs",
                "fn f() {\n/////*\nlet _ = ('\"','*');\nsee the password: Fake0Moo9Zed */\n}\n",
            ),
            (
                "a.ts",
                "x = 1;\n/////* open\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.c",
                "int x;\n/////* open\nsee the password: Fake0Moo9Zed */\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
}

/// Quotes that pair up in code around a mark that opens a block comment are
/// no proof that the mark is in a literal: two quotes of a regular expression,
/// of a JSX text or of a comment pair over it, and the comment is read. The
/// gate opens on every mark a literal seems to hold.
#[test]
fn a_pairing_of_quotes_over_a_block_comment_mark_does_not_hide_it() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "if (/a\"b/.test(c)) /* open \"\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.ts",
                "if (/a\"b/.test(c)) /* open \"x\"\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.ts",
                "const re = /\"x\"/; /* open \"\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.ts",
                "const t = <p>say \"hi\"</p>; /* open \"\nsee the password: Fake0Moo9Zed */\n",
            ),
            // No raw string outside the grammars that have one.
            (
                "a.ts",
                "if (/r\"/.test(c)) { /* open\nsee the password: Fake0Moo9Zed */ }\n",
            ),
            (
                "a.js",
                "x = a /R#\"/ b; /* open\nsee the password: Fake0Moo9Zed */\n",
            ),
            // A Rust character's escape is read whole.
            (
                "a.rs",
                "fn f() { let t = ('\\x41','\"'); /* open\nsee the password: Fake0Moo9Zed */ }\n",
            ),
            (
                "a.rs",
                "fn f() { let t = ['\\u{1}','\"','/']; // c\n/* open\nsee the password: Fake0Moo9Zed */ }\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    // In Rust a quote outside a comment always opens a literal, and a literal
    // that holds a mark hides it, so the mark opens no gate and nothing parses.
    // A quote that a literal or a comment holds in Rust still opens one, where
    // a line's literal runs on or a mark follows it.
    use crate::security::detector::code_syntax::{grammar_of, has_credential_assignment};
    let rust = grammar_of("a.rs").expect("a Rust grammar");
    for text in [
        "#[doc = \"in \\\"src/*.rs\\\" only\"]\npub input_tokens: i64,\n",
        "let a = \"src/*.rs\";\npub input_tokens: i64,\n",
        "let a = r#\"src/*.rs \"x\"\"#;\npub input_tokens: i64,\n",
    ] {
        assert!(!has_credential_assignment(text, rust), "{text}");
    }
    for text in [
        "let a = 1; /* open \"x\"\npub input_tokens: i64,\n",
        "let a = \"abc\ndef\" /* open \"\npub input_tokens: i64,\n",
        "let a = r#\"abc\ndef\"# /* open\npub input_tokens: i64,\n",
        "let a = r#\"abc\"# /* open \"\npub input_tokens: i64,\n",
        // A line comment holding a block opener is read apart by lexers, so
        // the mark opens the gate and the tree decides.
        "// say \"hi /* there\npub input_tokens: i64,\n",
    ] {
        assert!(has_credential_assignment(text, rust), "{text}");
    }
    hidden_in_files(
        &[
            (
                "a.rs",
                "let a = \"abc\ndef\" /* open \"\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.rs",
                "let a = r#\"abc\ndef\"# /* open\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.rs",
                "let a = '\"'; /* open \"\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.rs",
                "let a = \"src/*.rs\"; /* open\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.rs",
                "let a = \"*/\"; /* open\nsee the password: Fake0Moo9Zed */\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    // Where a quote may be no literal's (a regular expression, a JSX text), a
    // literal that holds a mark still opens the gate, and the file's tree
    // reads the comment or, finding none, leaves the code alone.
    let ts = grammar_of("a.ts").expect("a TypeScript grammar");
    for text in [
        "const a = \"src/*.rs\";\nconst b = { inputTokens: 1 };\n",
        "const a = \"*/\" + \"/*\";\nconst b = { inputTokens: 1 };\n",
    ] {
        assert!(has_credential_assignment(text, ts), "{text}");
    }
    kept_in_files(&[(
        "a.ts",
        "const a = \"src/*.rs\";\nconst b = { inputTokens: 1 };\n",
    )]);
    kept_in_files(&[
        ("a.rs", "let glob = \"**/*.rs\";\npub input_tokens: i64,\n"),
        (
            "a.rs",
            "fn f() { let a = \"src/*.rs\"; }\nstruct S {\n    input_tokens: i64,\n}\n",
        ),
    ]);
}

/// A block comment that nests (Rust, Scala) stays open past the first `*/`.
#[test]
fn a_nested_block_comment_is_read_to_its_outer_closer() {
    hidden_in_files(
        &[
            (
                "a.rs",
                "/* a /* b */\nsee the password: Fake0Moo9Zed */\nfn f() {}\n",
            ),
            (
                "a.scala",
                "/* a /* b */\nsee the password: Fake0Moo9Zed */\nobject A\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
}

/// Quotes that pair up over a block comment's mark, two strays in one line,
/// prove no literal: a line that can hold a stray quote parses.
#[test]
fn quotes_that_pair_over_a_block_comment_mark_do_not_hide_it() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "/\"/ /* open /\"/\nsee the password: Fake0Moo9Zed */\n",
            ),
            (
                "a.ts",
                "f('a\"b', /* open 'c\"d'\nsee the password: Fake0Moo9Zed */\n",
            ),
        ],
        &["Fake0Moo9Zed"],
    );
    // A glob in a literal, on a line with no construct that can hold a stray
    // quote, is still text.
    kept_in_files(&[("a.rs", "let glob = \"**/*.rs\";\npub input_tokens: i64,\n")]);
}

/// The gate looks at each byte of a line a bounded number of times: doubling
/// a line of key characters at most doubles the bytes it visits (a scan that
/// rereads the rest of the run from each word quadruples them).
#[test]
fn the_gate_visits_a_long_key_run_in_linear_work() {
    use crate::security::detector::code_syntax::{credential_assignment_scan, grammar_of};
    let grammar = grammar_of("a.ts").expect("a TypeScript grammar");
    for unit in ["pass", "pass-", "password", "Zq_token."] {
        let work = |units: usize| {
            let (found, visited) =
                credential_assignment_scan(&format!("// {}\n", unit.repeat(units)), grammar);
            assert!(!found, "no operator follows the run");
            visited
        };
        let (small, large) = (work(2_000), work(4_000));
        assert!(large < 3 * small, "{unit}: {small} bytes, then {large}");
    }
}

/// A literal a constructor call wraps (`Some("…")`, `String::from("…")`) is
/// the value the name holds, in a struct initializer as in a `let` (L1, B165
/// round 8); a call that reads another name stays readable.
#[test]
fn a_literal_wrapped_in_a_constructor_call_is_redacted() {
    let token = "q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc";
    hidden_in_files(
        &[
            (
                "a.rs",
                &format!("fn f() -> C {{ C {{\n    token: Some(\"{token}\"),\n}} }}\n"),
            ),
            (
                "a.rs",
                &format!("fn f() -> C {{ C {{\n    api_key: String::from(\"{token}\"),\n}} }}\n"),
            ),
            (
                "a.rs",
                "fn f() -> C { C {\n    password: Ok(\"Fake0Moo9Zed\".to_string()),\n} }\n",
            ),
            ("a.rs", "let password = Some(\"Fake0Moo9Zed\");\n"),
            (
                "a.ts",
                &format!("const c = {{ apiKey: String(\"{token}\") }};\n"),
            ),
        ],
        &[token, "Fake0Moo9Zed"],
    );
    kept_in_files(&[(
        "a.rs",
        "fn f() -> C { C {\n    token: Some(lookup(\"name\")),\n    key: Some(\"cache\"),\n} }\n",
    )]);
}

/// An escaped quote inside a value is the value's own character (B165 round
/// 10): the value read past it is hidden whole, not up to the escape.
#[test]
fn an_escaped_quote_inside_a_value_hides_the_value_past_it() {
    hidden_in_files(
        &[
            (
                "a.rs",
                "let c = \"Server=x;Password=Xy7\\\"Zq7xK2mPlanted9;\";\n",
            ),
            (
                "a.ts",
                "const c = \"Data Source=db;Pwd=Xy7\\\"Zq7xK2mPlanted8;\";\n",
            ),
        ],
        &["Zq7xK2mPlanted9", "Zq7xK2mPlanted8"],
    );
}

/// A literal's `\n` or `\t` escape ends a URL's host, so a URL after it is
/// read on its own and its password hidden (B165 round 10).
#[test]
fn a_url_after_an_escaped_line_break_has_its_password_hidden() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "const e = \"https://u:Moo16@h.com\\nhttps://u:Zq7xK2mPlanted9@h.com\";\n",
            ),
            (
                "a.ts",
                "const e = \"https://u:Moo16@h.com\\thttps://u:Zq7xK2mPlanted8@h.com\";\n",
            ),
            (
                "a.rs",
                "let e = \"REDIS_URL=redis://:Moo16@h:6379\\nAMQP_URL=amqp://u:Zq7xK2mPlanted7@h/vh\\n\";\n",
            ),
        ],
        &[
            "Moo16",
            "Zq7xK2mPlanted9",
            "Zq7xK2mPlanted8",
            "Zq7xK2mPlanted7",
        ],
    );
}

/// A literal whose backslash escapes nothing (a Rust raw string, a Go raw
/// string, a C# verbatim string) closes at its own delimiter: a value read in
/// it ends there, the closer stays, and the lines after it are read as the
/// source's own code (B165 round 10).
#[test]
fn a_raw_literal_keeps_its_closer_and_the_lines_after_it() {
    for (path, content, closer) in [
        (
            "a.rs",
            concat!(
                "fn f() {\n",
                "    let a = r\"Server=x;User Id=u;Password=Pw0rd9q\\\";\n",
                "    let b = (format!(\"jwt_secret = \\\"{value}\\\";\"), \"jwt_secret = \\\"\");\n",
                "}\n",
            ),
            "\";",
        ),
        (
            "a.rs",
            concat!(
                "fn f() {\n",
                "    let a = r#\"Server=x;User Id=u;Password=Pw0rd9q\\\"#;\n",
                "    let b = (format!(\"jwt_secret = \\\"{value}\\\";\"), \"jwt_secret = \\\"\");\n",
                "}\n",
            ),
            "\"#;",
        ),
        (
            "a.go",
            concat!(
                "package a\n",
                "var a = `Server=x;User Id=u;Password=Pw0rd9q\\`\n",
                "var b = []string{\"jwt_secret = \\\"{value}\\\";\", \"jwt_secret = \\\"\"}\n",
            ),
            "`",
        ),
        (
            "a.cs",
            concat!(
                "class A {\n",
                "    string a = @\"Server=x;User Id=u;Password=Pw0rd9q\\\";\n",
                "    string b = \"jwt_secret = \\\"\";\n",
                "}\n",
            ),
            "\";",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            let redacted = hides(content, scope, &["Pw0rd9q"]);
            let (lines, kept): (Vec<_>, Vec<_>) =
                (redacted.lines().collect(), content.lines().collect());
            assert!(
                lines[1].ends_with(&format!("]{closer}")),
                "{scope:?}: closer lost: {redacted}"
            );
            assert_eq!(lines[2..], kept[2..], "{scope:?}: {redacted}");
        }
    }
}

/// A key block written in pieces (`concat!` of one literal per line) is one
/// value across them: the literal close of its first piece does not end it,
/// and no line of its body is shown (B165 round 10).
#[test]
fn a_key_block_written_in_literal_pieces_stays_hidden() {
    let body = [
        "MIIEowIBAAKCAQEAq8Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5WaVo0MiExDzlUtQ8",
        "Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5WaVo0MiExDzlUtq8Zr4LmT2vXw9KpN7s",
        "Yd3HfJ6gBc1QeRu5WaVo0MiExDzlUtQ8Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5",
    ];
    let content = format!(
        "const KEY: &str = concat!(\n    \"-----BEGIN RSA PRIVATE KEY-----\\n\",\n    \"{}\\n\",\n    \"{}\\n\",\n    \"{}\\n\",\n    \"-----END RSA PRIVATE KEY-----\\n\",\n);\n",
        body[0], body[1], body[2]
    );
    hidden_in_files(&[("a.rs", &content)], &body);
}

/// A literal's escaped line break (`\n`) ends the line the program holds,
/// and a connection string's value with it: the next line's fields are
/// shown. Where the backslash escapes nothing (a raw string) the value runs
/// on (B190 item 2).
#[test]
fn an_escaped_line_break_ends_a_connection_string_value() {
    let content = "fn f() {\n    let s = \"CONN=Server=db;Password=Hunter2x\\nNEXT=Server=db;User=sa\\n\";\n}\n";
    for scope in [Scope::File("a.rs"), Scope::Stored(Some("a.rs"))] {
        let redacted = hides(content, scope, &["Hunter2x"]);
        assert!(
            redacted.contains("]\\nNEXT=Server=db;User=sa\\n\";"),
            "{scope:?}: {redacted}"
        );
    }
    // The text past the clip is searched again: a field there is judged.
    let next = "fn f() {\n    let s = \"CONN3=Server=db;User=sa;Password=x\\nCONN4=Pwd=ab12;Server=db\\n\";\n}\n";
    hidden_in_files(&[("a.rs", next)], &["ab12"]);
    let raw = "fn f() {\n    let s = r\"CONN=Server=db;Password=Hunter2x\\nNEXT=Hunter3y\";\n}\n";
    hidden_in_files(&[("a.rs", raw)], &["Hunter2x", "Hunter3y"]);
}

/// A config line written in a one-line literal (`"password = …"`), or after
/// one of its escaped line breaks, is read from where the literal's line
/// starts, as the same line in a multi-line literal is: the code before the
/// opening quote is no part of its key, and the closer stays (B190, R6 N1).
#[test]
fn a_config_line_in_a_one_line_literal_is_read_from_the_literal_start() {
    for (path, content) in [
        (
            "a.rs",
            "fn f() {\n    let s = \"password = Zq7xK2mPlanted9\";\n}\n",
        ),
        (
            "a.py",
            "def f():\n    y = \"jwt_secret = Zq7xK2mPlanted9\"\n",
        ),
        (
            "a.rs",
            "fn f() {\n    let t = \"password: CORRECT_HORSE_BATTERY_STAPLE\";\n}\n",
        ),
        (
            "a.rs",
            "fn f() {\n    let u = \"a=1\\npassword = Zq7xK2mPlanted9\";\n}\n",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            let redacted = hides(
                content,
                scope,
                &["Zq7xK2mPlanted9", "CORRECT_HORSE_BATTERY_STAPLE"],
            );
            assert!(
                redacted
                    .lines()
                    .nth(1)
                    .is_some_and(|line| line.trim_end_matches(';').ends_with("]\"")),
                "{scope:?}: closer lost: {redacted}"
            );
        }
    }
}

/// The last line of a literal written over several lines ends at its
/// closer, with code after it on the same physical line
/// (`password = …";`): a rule anchored at the line's end does not reach it
/// there, so it is read up to the closer (B190, the R6 N1 class).
#[test]
fn the_last_line_of_a_multiline_literal_is_read_up_to_its_closer() {
    for (path, content) in [
        (
            "a.rs",
            "fn f() {\n    let u = \"a=1\npassword = Zq7xK2mPlanted9\";\n}\n",
        ),
        ("a.ts", "const u = `a=1\npassword: Zq7xK2mPlanted9`;\n"),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            let redacted = hides(content, scope, &["Zq7xK2mPlanted9"]);
            assert!(
                redacted.contains("]\";") || redacted.contains("]`;"),
                "{scope:?}: closer lost: {redacted}"
            );
        }
    }
}

/// A literal's line is a literal of code first: its value is a credential
/// only as a code literal is one (long enough, or a chosen secret), a
/// file-class rule does not read it as a dotenv file, and a line written
/// with escapes (`\"`) is not read as one (B190, the measured cost of R6 N1).
#[test]
fn a_config_line_in_a_one_line_literal_is_judged_as_a_code_literal() {
    for path in ["a.rs", "a.ts"] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            for content in [
                "f(\"GITHUB_TOKEN: set\");\n",
                "f('GH_TOKEN=project-secret');\n",
                "f(\"KEY=one\\n\");\n",
                "f(\"PASS: installed and validated the archive\");\n",
                "f(\"  access_token: \\\"replace with your access token\\\"\\n\");\n",
            ] {
                keeps(content, scope);
            }
        }
    }
}

/// Shell has no grammar the engine parses, so a backslash in a `.sh` file
/// counts as an escape: a single-quoted value ending in one takes the quote,
/// and the value stays hidden (fail closed; B190 item 3, a pin, no fix).
#[test]
fn a_shell_single_quoted_value_stays_hidden() {
    hidden_in_files(
        &[("a.sh", "export DB_PASSWORD='Zq7xK2mPlanted9\\'\nNEXT=1\n")],
        &["Zq7xK2mPlanted9"],
    );
}

/// A literal that holds a backslash as text (a raw string, a C# verbatim
/// string, a Go raw string, a shell or Python raw single-quoted value) may
/// end in one: its value closes at the quote after the backslash and is
/// hidden. Read with escapes, that quote is escaped and no literal closes,
/// so the value was shown (B190, found by the item 3 pin).
#[test]
fn a_literal_ending_in_a_backslash_it_does_not_escape_is_hidden() {
    hidden_in_files(
        &[
            (
                "a.rs",
                "fn f() {\n    let api_key = r\"Zq7xK2mPlanted9\\\";\n}\n",
            ),
            (
                "a.rs",
                "fn f() {\n    let api_key = r#\"Zq7xK2mPlanted9\\\"#;\n}\n",
            ),
            ("a.go", "package a\nvar apiKey = `Zq7xK2mPlanted9\\`\n"),
            (
                "a.cs",
                "class A {\n    string apiKey = @\"Zq7xK2mPlanted9\\\";\n}\n",
            ),
            ("a.py", "API_KEY = r'Zq7xK2mPlanted9\\\\'\n"),
            ("a.sh", "DB_PASSWORD='Zq7xK2mPlanted9\\'\n"),
        ],
        &["Zq7xK2mPlanted9"],
    );
}

/// RV pins (round 2): an escaped backslash before `n` is no line break, so
/// the whole password stays hidden; a single-quoted bytes literal under a
/// credential name is hidden.
#[test]
fn rv_an_escaped_backslash_before_n_and_a_bytes_literal_stay_hidden() {
    for scope in [Scope::File("a.rs"), Scope::Stored(Some("a.rs"))] {
        hides(
            "fn f() { let s = \"Server=db;Password=ab\\\\nZq7xK2mPlanted9;\"; }\n",
            scope,
            &["Zq7xK2mPlanted9"],
        );
    }
    for scope in [Scope::File("a.py"), Scope::Stored(Some("a.py"))] {
        hides(
            "secret_key = b'Zq7xK2mPlanted9xx'\n",
            scope,
            &["Zq7xK2mPlanted9xx"],
        );
    }
}
