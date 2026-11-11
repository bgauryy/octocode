//! The scan's shortcuts against the scan itself: a line the block scan
//! skips, a line the gate does not look for a comment in, a kind of node
//! read once and a credential word found by one automaton each say exactly
//! what the full read of the same text says. Checked on every line of the
//! engine's own sources, in every state a line can start in, and on
//! generated token soup.

use super::*;

/// [`block_after`] with no line skipped: the scan of every line.
fn full_block_after(line: &str, before: BlockState, grammar: &LanguageEntry) -> BlockState {
    match lexicon(grammar) {
        Lexicon::Rust => rust_block_after(line, before),
        Lexicon::LineHash => BlockState::default(),
        Lexicon::Generic => {
            let mut after = generic_block_after(line, before);
            after.continued = line.ends_with('\\') && (before.continued || line.contains("//"));
            after
        }
    }
}

/// [`names_a_credential_word`] as its words spell it: each looked for in the
/// lowered text.
fn full_names_a_credential_word(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    crate::security::patterns::CREDENTIAL_WORDS
        .split('|')
        .any(|credential| lower.contains(credential))
}

/// [`has_credential_assignment`] with every line read for its comment and
/// its keys.
fn full_credential_assignment(text: &str, grammar: &LanguageEntry) -> bool {
    let words: Vec<&str> = crate::security::patterns::CREDENTIAL_WORDS
        .split('|')
        .collect();
    let mut visited = 0;
    let mut block = BlockState::default();
    text.lines().any(|line| {
        let lead_end = comment_start(line, block.is_open(), grammar);
        block = full_block_after(line, block, grammar);
        line_assigns_credential(line, lead_end, &words, &mut visited)
    })
}

/// [`has_quoted_credential_assignment`] with every quoted line read for its
/// keys.
fn full_quoted_credential_assignment(text: &str) -> bool {
    let words: Vec<&str> = crate::security::patterns::CREDENTIAL_WORDS
        .split('|')
        .collect();
    let mut visited = 0;
    text.lines().any(|line| {
        line.contains(['"', '\'', '`']) && line_assigns_credential(line, 0, &words, &mut visited)
    })
}

/// [`data_ranges`] with every node's kind read by name.
fn full_data_ranges(root: Node<'_>) -> Vec<(Range<usize>, bool)> {
    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let data = holds_data(node);
        if data {
            ranges.push((
                node.byte_range(),
                node.is_named() && node.kind().contains("comment"),
            ));
        }
        if !data && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return ranges;
            }
        }
    }
}

/// Every state a line can start in, and some no scan reaches: a shortcut
/// holds in each or it is no shortcut.
fn states() -> Vec<BlockState> {
    let mut states = Vec::new();
    for depth in [0, 1, 3] {
        for literal in [
            Literal::None,
            Literal::Quoted,
            Literal::Raw(0),
            Literal::Raw(2),
        ] {
            for (triple, continued, hidden) in [
                (false, false, false),
                (true, false, false),
                (false, true, false),
                (false, false, true),
                (true, true, true),
            ] {
                states.push(BlockState {
                    depth,
                    triple,
                    literal,
                    continued,
                    hidden,
                });
            }
        }
    }
    states
}

fn grammars() -> [&'static LanguageEntry; 3] {
    ["a.rs", "a.ts", "a.py"].map(|path| grammar_of(path).expect("a grammar"))
}

/// The engine's own sources, as texts.
fn engine_sources() -> Vec<String> {
    let mut files = Vec::new();
    rust_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files.sort();
    files
        .iter()
        .map(|file| std::fs::read_to_string(file).expect("a UTF-8 source"))
        .collect()
}

/// A sequence of numbers a test can repeat.
struct Sequence(u64);

impl Sequence {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % below
    }
}

/// What a scan's marks, a gate's keys and their near misses are made of.
const TOKENS: &[&str] = &[
    "\"",
    "'",
    "`",
    "/",
    "//",
    "/*",
    "*/",
    "*",
    "#",
    "#[",
    "#!",
    "--",
    ";",
    "%",
    "<!--",
    "-->",
    "\"\"\"",
    "'''",
    "(*",
    "\\",
    "r",
    "r#\"",
    "\"#",
    "br\"",
    "b",
    "'a",
    "'\\''",
    "\\\"",
    " ",
    "\t",
    "=",
    ":",
    "==",
    "::",
    "x",
    "_",
    ".",
    "-",
    "é",
    "ſ",
    "password",
    "PASSWORD",
    "PassWord",
    "pass",
    "pwd",
    "key",
    "Key",
    "token",
    "secret",
    "credential",
    "passphrase",
    "passwd",
    "pas",
    "tok",
    "ke",
    "y",
    "db_",
    "[0]",
    "0",
    "\n",
    "\r\n",
    "\n",
];

/// Generated texts of a few lines each.
fn soup(cases: usize) -> Vec<String> {
    let mut sequence = Sequence(0x005e_ed0f_7e57);
    (0..cases)
        .map(|_| {
            let tokens = 1 + sequence.next(24);
            (0..tokens)
                .map(|_| TOKENS[sequence.next(TOKENS.len())])
                .collect()
        })
        .collect()
}

/// Every soup line in every state, and every line of the engine's sources
/// in the state a text starts in and one other, each state in its turn.
#[test]
fn a_skipped_line_leaves_the_state_the_full_scan_leaves() {
    let states = states();
    let same = |line: &str, before: BlockState| {
        for grammar in grammars() {
            assert_eq!(
                block_after(line, before, grammar),
                full_block_after(line, before, grammar),
                "{:?} after {before:?}: {line:?}",
                lexicon(grammar),
            );
        }
    };
    let mut lines = 0;
    for text in soup(6_000) {
        for line in text.lines() {
            lines += 1;
            states.iter().for_each(|&before| same(line, before));
        }
    }
    assert!(lines > 6_000, "only {lines} soup lines were compared");
    for text in engine_sources() {
        for line in text.lines() {
            lines += 1;
            same(line, BlockState::default());
            same(line, states[lines % states.len()]);
        }
    }
    assert!(lines > 100_000, "only {lines} lines were compared");
}

/// The gates say of a text what reading every line of it says: of each soup
/// text, and of each window of a few lines of the engine's sources (a whole
/// source answers at its first hit, so its later lines would go unasked).
#[test]
fn the_gates_answer_as_a_read_of_every_line_answers() {
    let sources = engine_sources();
    let windows = sources.iter().flat_map(|text| {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        (0..lines.len())
            .step_by(3)
            .map(|at| lines[at..(at + 4).min(lines.len())].concat())
            .collect::<Vec<_>>()
    });
    let (mut texts, mut assigned, mut quoted) = (0, 0, 0);
    for text in windows.chain(soup(60_000)) {
        texts += 1;
        for grammar in grammars() {
            let full = full_credential_assignment(&text, grammar);
            assigned += usize::from(full);
            assert_eq!(
                has_credential_assignment(&text, grammar),
                full,
                "{:?}: {text:?}",
                lexicon(grammar)
            );
        }
        let full = full_quoted_credential_assignment(&text);
        quoted += usize::from(full);
        assert_eq!(has_quoted_credential_assignment(&text), full, "{text:?}");
    }
    assert!(texts > 100_000, "only {texts} texts were compared");
    assert!(
        assigned > 1_000 && quoted > 1_000,
        "few texts assign a credential: {assigned} in a comment, {quoted} beside a quote"
    );
}

#[test]
fn one_automaton_names_the_credential_words_their_spelling_names() {
    let words = crate::security::patterns::CREDENTIAL_WORDS;
    assert!(
        words
            .split('|')
            .all(|word| !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_lowercase())),
        "the automaton reads lowercase ASCII words in any ASCII case: {words}"
    );
    let mut named = 0;
    for text in engine_sources().iter().chain(&soup(60_000)) {
        for line in text.lines() {
            let full = full_names_a_credential_word(line);
            named += usize::from(full);
            assert_eq!(names_a_credential_word(line), full, "{line:?}");
            for word in line.split_whitespace() {
                assert_eq!(
                    names_a_credential_word(word),
                    full_names_a_credential_word(word),
                    "{word:?}"
                );
            }
        }
    }
    assert!(named > 1_000, "only {named} lines name a credential word");
}

#[test]
fn a_kind_read_once_places_the_data_every_node_read_by_name_places() {
    let samples = [
        (
            "a.ts",
            "// c\nconst a = `t ${b} \"q\"`; /* d */\nlabel: for (;;) { f('x', \"y\\n\"); }\n",
        ),
        (
            "a.py",
            "# c\ndef f():\n    \"\"\"doc\"\"\"\n    return f'{a} b', \"c\"\n",
        ),
        ("a.go", "package a\n// c\nvar s = `raw` + \"q\" /* d */\n"),
        ("a.ts", "const broken = \"unclosed\nfunction ( {\n"),
    ];
    let sources = engine_sources();
    let texts = samples
        .iter()
        .map(|(path, text)| (*path, *text))
        .chain(sources.iter().map(|text| ("a.rs", text.as_str())));
    let mut spans = 0;
    for (path, text) in texts {
        let grammar = grammar_of(path).expect("a grammar");
        let tree =
            crate::signatures::extractor::parse_until_stopped(text, &grammar.language, &|| false)
                .expect("a tree");
        let found: Vec<(Range<usize>, bool)> = data_ranges(tree.root_node())
            .into_iter()
            .map(|span| (span.range, span.comment))
            .collect();
        spans += found.len();
        assert_eq!(found, full_data_ranges(tree.root_node()), "{path}");
    }
    assert!(spans > 10_000, "only {spans} data spans were compared");
}
