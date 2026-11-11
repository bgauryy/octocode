//! What the syntax of a source-code file says about a line a configuration rule
//! matched there: in the code itself (`f.evidence.passed = true;`,
//! `dispatchToken: z.string().optional(),`) it is a statement or a member, and no
//! credential; in a string literal or a comment it is text, which may hold a
//! config file (`const FIXTURE = "password: hunter2";`) and is judged as one.
//!
//! The parse is the language grammar's own, not a list of names: the text is
//! data when a node of the match's ancestry is a string or a comment, or a
//! label (`password: hunter2` as a statement is how YAML or INI text pasted
//! into a source file parses). A parse that fails or holds any error or
//! missing node counts as data for the whole text, so nothing is left
//! readable on a guess.

use std::cell::OnceCell;
use std::ops::Range;

use tree_sitter::{Node, Tree};

use crate::signatures::languages::{LanguageEntry, find_entry};

/// The grammar of the source-code file at `path`: its extension belongs to a
/// language the engine parses.
pub(super) fn grammar_of(path: &str) -> Option<&'static LanguageEntry> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    find_entry(&ext.to_ascii_lowercase())
}

/// The parse of one source-code text, made when first asked.
pub(crate) struct CodeSyntax<'a> {
    text: &'a str,
    grammar: &'static LanguageEntry,
    tree: OnceCell<Option<Tree>>,
    lines: OnceCell<Vec<CommentLine>>,
    data: OnceCell<Vec<DataSpan>>,
    literal_lines: OnceCell<Vec<Range<usize>>>,
}

/// Where the grammar places the first character of a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Place {
    /// Outside every string and comment: a statement or a member.
    Code,
    /// In a comment: prose before it is a config file.
    Comment,
    /// In a string, or anywhere in a text that does not parse: text that may
    /// hold a config file.
    Text,
}

/// One outermost node that holds data ([`holds_data`]).
struct DataSpan {
    range: Range<usize>,
    comment: bool,
}

#[cfg(test)]
thread_local! {
    /// The parses this thread made ([`CodeSyntax::parse`]).
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many texts this thread has parsed: a test reads what one judgment costs.
#[cfg(test)]
pub(super) fn parses() -> usize {
    PARSES.with(std::cell::Cell::get)
}

impl<'a> CodeSyntax<'a> {
    /// `None` when `path` names no source-code file: its text is no code.
    pub(super) fn of(path: &str, text: &'a str) -> Option<Self> {
        Some(Self {
            text,
            grammar: grammar_of(path)?,
            tree: OnceCell::new(),
            lines: OnceCell::new(),
            data: OnceCell::new(),
            literal_lines: OnceCell::new(),
        })
    }

    /// The parse, when the text parses without an error or missing node.
    fn parse(&self) -> Option<&Tree> {
        self.tree
            .get_or_init(|| {
                #[cfg(test)]
                PARSES.with(|count| count.set(count.get() + 1));
                crate::signatures::extractor::parse_until_stopped(
                    self.text,
                    &self.grammar.language,
                    &|| false,
                )
                .ok()
                .filter(|tree| !tree.root_node().has_error())
            })
            .as_ref()
    }

    /// The comment lines a credential can start in: each holds a key with a
    /// credential word that an operator follows (`password`, `DB_PASS`), and
    /// is text, whatever precedes it on its line (`x = 1; // password: …`,
    /// `// set the password: …`), less the comment's closer (`*/`). A rule that
    /// reads configuration reads it as a line of its own, from each of its keys.
    /// A text that does not parse is read lexically, from each place a comment
    /// may open.
    pub(super) fn comment_lines(&self) -> &[CommentLine] {
        self.lines.get_or_init(|| {
            let mut lines = Vec::new();
            // No parse for a text no comment can carry a credential in.
            if has_credential_assignment(self.text, self.grammar) {
                match self.parse() {
                    Some(tree) => collect_comment_lines(self.text, tree.root_node(), &mut lines),
                    // No tree tells a comment from code: the lines read as
                    // comments where one may open, so nothing is left readable.
                    None => lexical_comment_lines(self.text, self.grammar, &mut lines),
                }
            }
            lines
        })
    }

    /// Where the grammar places the match at `range`. The match can run past
    /// the end of a string that opens before it (`` `\npassword: x` ``), so the
    /// verdict is the grammar's at the match's first character, past any
    /// indent: a match that starts in a literal or a comment is never code. A
    /// text that does not parse is [`Place::Text`] throughout.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "`range` is a regex match; `start` follows trimmed whitespace and `end` adds one char's UTF-8 length"
        )
    )]
    pub(super) fn place_of(&self, range: &Range<usize>) -> Place {
        let matched = &self.text[range.clone()];
        let start = range.start + (matched.len() - matched.trim_start().len());
        let end = start + self.text[start..].chars().next().map_or(0, char::len_utf8);
        let Some(tree) = self.parse() else {
            return Place::Text;
        };
        // A node holds the first character when it spans it; the nodes that do
        // form one chain from the root, so some of them is data exactly when a
        // data range spans it. The ranges are those of the outermost data
        // nodes, in order and apart, so the one range that can span the
        // character is the last that starts at or before it: a search of a
        // sorted list, where a walk down from the root reads every child of a
        // node with thousands of them (a file of comments) for each match.
        let data = self.data.get_or_init(|| data_ranges(tree.root_node()));
        let before = data.partition_point(|span| span.range.start <= start);
        match before
            .checked_sub(1)
            .and_then(|last| data.get(last))
            .filter(|span| end <= span.range.end)
        {
            None => Place::Code,
            Some(span) if span.comment => Place::Comment,
            Some(_) => Place::Text,
        }
    }
}

impl CodeSyntax<'_> {
    /// Whether the backslash at `at` escapes the character after it, as the
    /// grammar reads it: an escape sequence of a string. A raw string, a Go
    /// raw or shell single-quoted literal, a C# verbatim string or a comment
    /// holds the backslash as text. Outside every string and comment, and in
    /// a text that does not parse, it counts as an escape (fail closed: the
    /// secret it ends takes one more character).
    pub(super) fn escapes_at(&self, at: usize) -> bool {
        let Some(tree) = self.parse() else {
            return true;
        };
        let Some(mut node) = tree.root_node().descendant_for_byte_range(at, at + 1) else {
            return true;
        };
        loop {
            if node.kind().contains("escape") {
                return true;
            }
            if holds_data(node) {
                return false;
            }
            match node.parent() {
                Some(parent) => node = parent,
                None => return true,
            }
        }
    }

    /// Whether the grammar reads an escape sequence that starts with the
    /// backslash at `at`. Unlike [`CodeSyntax::escapes_at`] this fails toward
    /// no escape: without a parse, in a raw string or a comment, and for a
    /// backslash inside another escape (`\\"`) it is `false`, so a caller
    /// reads the escape's character only where the source has one.
    pub(super) fn escape_starts_at(&self, at: usize) -> bool {
        self.parse()
            .and_then(|tree| tree.root_node().descendant_for_byte_range(at, at + 1))
            .is_some_and(|node| node.start_byte() == at && node.kind().contains("escape"))
    }

    /// The lines of text the string literals hold that a rule anchored at a
    /// line's start and end does not reach, each up to where it ends: the
    /// literal's first line from its opening delimiter
    /// (`let s = "password = …";`), each line after an escaped line break
    /// (`"a=1\npassword = …"`), and the last line of a literal over several
    /// lines, which ends at the closer (`password = …";`). Each runs to the
    /// next line break, escaped or not, or the closer. The other lines of a
    /// literal start and end as lines do. Only a
    /// line with a key that holds a credential word and an operator after it
    /// is listed ([`names_a_credential_word`]). None without a parse: there
    /// the whole text is text already ([`Place::Text`]).
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "offsets are node bounds, ASCII delimiters and line breaks: char boundaries of the text"
        )
    )]
    pub(super) fn literal_lines(&self) -> &[Range<usize>] {
        self.literal_lines.get_or_init(|| {
            // No parse for a text no literal can hold such a line in.
            if !has_quoted_credential_assignment(self.text) {
                return Vec::new();
            }
            let Some(tree) = self.parse() else {
                return Vec::new();
            };
            let data = self.data.get_or_init(|| data_ranges(tree.root_node()));
            let mut lines = Vec::new();
            for span in data.iter().filter(|span| !span.comment) {
                let literal = &self.text[span.range.clone()];
                let (Some(open), Some(close)) = (opener_len(literal), closer_len(literal)) else {
                    continue;
                };
                let body = span.range.start + open..span.range.end - close;
                if body.is_empty() {
                    continue;
                }
                let mut starts = vec![body.start];
                // The last line of a literal over several lines starts at a
                // line's start but ends at the closer, with code after it.
                if let Some(at) = self.text[body.clone()].rfind(['\n', '\r']) {
                    starts.push(body.start + at + 1);
                }
                if let Some(node) = tree
                    .root_node()
                    .descendant_for_byte_range(span.range.start, span.range.end)
                {
                    line_break_escapes(self.text, node, &mut starts);
                }
                for start in starts {
                    let end = self.literal_line_end(start, body.end);
                    let line = &self.text[start..end];
                    if names_a_credential_word(line) && line.contains(['=', ':']) {
                        lines.push(start..end);
                    }
                }
            }
            lines.sort_by_key(|line| line.start);
            lines
        })
    }

    /// Where the literal line from `start` ends, at most at `end` (the
    /// literal's closer): its first line break, or the first backslash that
    /// escapes one ([`is_line_break_escape`]). Any other escape is the
    /// line's, its escaped character too (`\\n` is a backslash and an `n`).
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "`at` is an ASCII backslash's offset, a char boundary of the text"
        )
    )]
    fn literal_line_end(&self, start: usize, end: usize) -> usize {
        let bytes = self.text.as_bytes();
        let mut at = start;
        while at < end {
            match bytes[at] {
                b'\n' | b'\r' => return at,
                b'\\' if self.escape_starts_at(at) && is_line_break_escape(&self.text[at..]) => {
                    return at;
                }
                b'\\' => at += 2,
                _ => at += 1,
            }
        }
        end
    }

    /// Where the string literal the grammar places byte `at` in closes: the
    /// start of its closing delimiter, or of the last piece's when literals
    /// are joined into one value (`"Fake0" + "Moo9Zed"`, `"a" "b"`: only blanks
    /// and one `+` between them, as [`super::credential_values`] joins pieces).
    /// A value a rule read past it to `until` (a config line to its end:
    /// `"…Password=x",` or `` `…Pwd=x` ``) ends there when what it took past
    /// the literal holds no other string or comment: it took the closer and
    /// code. Past another literal or a comment the read may be one value
    /// written across them (a key block in `concat!` pieces), so it keeps its
    /// extent (fail closed). `None` then, outside a string (in a comment, in
    /// code), or without a parse.
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::string_slice,
            reason = "a data span is a node's byte range, char boundaries of the text"
        )
    )]
    pub(super) fn literal_end(&self, at: usize, until: usize) -> Option<usize> {
        let tree = self.parse()?;
        let data = self.data.get_or_init(|| data_ranges(tree.root_node()));
        let mut index = data
            .partition_point(|span| span.range.start <= at)
            .checked_sub(1)?;
        let first = data.get(index)?;
        if at >= first.range.end || first.comment {
            return None;
        }
        let mut close = first.range.end - closer_len(&self.text[first.range.clone()])?;
        if at >= close {
            return None;
        }
        while let Some(next) = data.get(index + 1).filter(|next| !next.comment) {
            let seam = self.text[data[index].range.end..next.range.start].trim();
            if !(seam.is_empty() || seam == "+") {
                break;
            }
            let Some(len) = closer_len(&self.text[next.range.clone()]) else {
                break;
            };
            close = next.range.end - len;
            index += 1;
        }
        let past = data
            .get(index + 1)
            .is_some_and(|next| next.range.start < until);
        (!past).then_some(close)
    }
}

/// The length of the delimiter that closes `literal`, the whole text of a
/// string node: the quotes that open it after any prefix (`r`, `b`, `f`, `@`,
/// `$`), with a raw string's `#`s mirrored (`r#"` closes with `"#`), when the
/// literal ends with them. `""` is an empty literal, not a triple quote's
/// start. `None` when it opens with no quote (a label) or ends without its
/// closer.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are trimmed lengths of ASCII marks at the text's start"
    )
)]
fn closer_len(literal: &str) -> Option<usize> {
    let body =
        literal.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '@' | '$'));
    let hashes = body.len() - body.trim_start_matches('#').len();
    let quoted = &body[hashes..];
    let quote = quoted
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let run = quoted.len() - quoted.trim_start_matches(quote).len();
    let quotes = if run == 2 { 1 } else { run };
    let closer = format!("{}{}", quote.to_string().repeat(quotes), "#".repeat(hashes));
    (body.len() >= 2 * closer.len() && body.ends_with(&closer)).then_some(closer.len())
}

/// The length of the delimiter that opens `literal`, the whole text of a
/// string node: its prefix (`r`, `b`, `f`, `@`, `$`), a raw string's `#`s and
/// its quotes, as many as [`closer_len`] closes it with. `None` when it has no
/// closer.
fn opener_len(literal: &str) -> Option<usize> {
    let body =
        literal.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '@' | '$'));
    Some(literal.len() - body.len() + closer_len(literal)?)
}

/// Whether `text` starts with an escape of a line break (`\n`, `\r`).
fn is_line_break_escape(text: &str) -> bool {
    text.starts_with("\\n") || text.starts_with("\\r")
}

/// Appends where each line after an escaped line break under `node` starts:
/// the end of each escape node that writes one ([`is_line_break_escape`]). A
/// walk with a cursor, each node visited once.
fn line_break_escapes(text: &str, node: Node<'_>, starts: &mut Vec<usize>) {
    let mut cursor = node.walk();
    loop {
        let current = cursor.node();
        let escape = current.kind().contains("escape");
        if escape
            && text
                .get(current.start_byte()..)
                .is_some_and(is_line_break_escape)
        {
            starts.push(current.start_byte() + 2);
        }
        if !escape && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.node() == node {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

/// The byte ranges of the outermost nodes under `root` that hold data
/// ([`holds_data`]), in text order. A walk with a cursor, so a deeply nested
/// tree does not overflow the call stack, and each node is visited once.
fn data_ranges(root: Node<'_>) -> Vec<DataSpan> {
    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let data = holds_data(node);
        if data {
            ranges.push(DataSpan {
                range: node.byte_range(),
                comment: node.is_named() && node.kind().contains("comment"),
            });
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

/// What opens a comment in some grammar the engine parses, or a string a
/// comment may be written in (a docstring). A comment line holds one of these
/// before its text, or sits inside a block comment that opened earlier.
const COMMENT_OPENERS: [&str; 10] = [
    "//", "#", "--", ";", "%", "/*", "<!--", "\"\"\"", "'''", "(*",
];

/// What a text has opened and not closed by some line: block comments
/// (`/* … */`, `<!-- … -->`) counted by nesting depth, and a triple-quoted
/// string. Block comments nest in Rust and Scala, and a nesting count only
/// ever holds a comment open longer than a grammar that does not nest would,
/// which costs a parse and never leaves a comment's text readable.
#[derive(Clone, Copy, Default)]
struct BlockState {
    depth: u32,
    triple: bool,
    /// The literal a line ended in, which the next one continues
    /// ([`rust_block_after`]); no other grammar's scan sets it.
    literal: Literal,
    /// The line ended in a `\` after a `//` comment opened on it or on a line
    /// it continues: C's line comment runs on over the line end.
    continued: bool,
    /// A Rust scan met a place grammars read apart ([`holds_disputed_opener`]),
    /// so no literal state after it is certain: from there on it reads every
    /// mark as plain text ([`scan_block_marks`] without strings), which reads
    /// every comment any of the readings has, and the tree decides.
    hidden: bool,
}

/// How a line scan tells a grammar's comments from its code and literals: the
/// one lexical property each grammar declares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lexicon {
    /// Rust, read exactly by [`rust_block_after`]: every `"` outside a
    /// comment opens a literal, and the scan models each form that holds one
    /// (character literals with any escape, raw strings, strings that run over
    /// line ends), nested block comments and `//` comments. `#[` and `#!` open
    /// an attribute, not a comment.
    Rust,
    /// A grammar whose only comment runs from `#` to its line's end (Python):
    /// no comment runs over a line end, so no line's state reaches the next.
    LineHash,
    /// Every other grammar: block comments counted by nesting, and literals
    /// that a quote may or may not open (a regular expression's `/"/`, a
    /// character `'"'`, a template), so a pairing of quotes proves nothing and
    /// a mark it would hide is read as a mark ([`block_after`]).
    Generic,
}

fn lexicon(grammar: &LanguageEntry) -> Lexicon {
    match grammar.language_id {
        Some("rust") => Lexicon::Rust,
        Some("python") => Lexicon::LineHash,
        _ => Lexicon::Generic,
    }
}

/// A literal that runs over line ends in Rust.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Literal {
    #[default]
    None,
    /// A double-quoted string.
    Quoted,
    /// A raw string (`r#"…"#`) opened with this many `#`.
    Raw(usize),
}

impl BlockState {
    fn is_open(self) -> bool {
        self.depth > 0 || self.triple || self.continued
    }
}

/// The state after `line`, given the state before it.
fn block_after(line: &str, before: BlockState, grammar: &LanguageEntry) -> BlockState {
    match lexicon(grammar) {
        // Rust has no construct but a literal, a character or a comment that
        // holds a double quote, and its scan reads each of them exactly, so a
        // literal that hides a mark hides it: no pairing is in doubt.
        Lexicon::Rust => rust_block_after(line, before),
        Lexicon::LineHash => BlockState::default(),
        Lexicon::Generic => {
            let mut after = generic_block_after(line, before);
            after.continued = line.ends_with('\\') && (before.continued || line.contains("//"));
            after
        }
    }
}

/// [`block_after`] for a [`Lexicon::Generic`] grammar.
fn generic_block_after(line: &str, before: BlockState) -> BlockState {
    let marks = scan_block_marks(line, before, true);
    // A double quote may be no string's: `/"/.test(x)`, `a"b` inside a
    // regular expression, `'He said "hi'`, a quote in a comment. A literal
    // hides every mark in it, and nothing on the line proves that a pair of
    // quotes is a literal, so the gate is closed on no pairing: a quote that
    // nothing closes, or a pairing over a mark that opens a block comment,
    // reads the line's marks again as plain text, once. Opening on a mark
    // that was in a literal costs a parse of a text with a key and an
    // operator, and a block comment that opens after a quote is never missed.
    // A line comment that holds a block opener is read apart by grammars
    // ([`holds_disputed_opener`]), so its mark counts too.
    if marks.in_string || marks.hidden || holds_disputed_opener(line) {
        return scan_block_marks(line, before, false).state;
    }
    marks.state
}

/// Whether `line` holds a `//` and, after it, a mark that opens a block
/// comment: a place lexers read apart. tree-sitter-rust opens a block at
/// `/////*`, rustc reads a line comment to the line's end, and a C
/// preprocessor joins lines before it reads comments. A scan reads the mark
/// as an opener (fail closed): a comment no reading has costs a parse, and
/// the tree decides.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "`at` is a `find` hit of the ASCII `//`, two bytes wide"
    )
)]
fn holds_disputed_opener(line: &str) -> bool {
    line.find("//")
        .is_some_and(|at| line[at + "//".len()..].contains("/*"))
}

/// The state after a line of Rust, given the state before it: block comments
/// by nesting depth, and a string or raw string that runs over the line end
/// (which the next line continues, so its quotes pair as the grammar's do). A
/// `//` comment ends the scan of its line, so a quote or a `/*` in it opens
/// nothing.
fn rust_block_after(line: &str, mut state: BlockState) -> BlockState {
    if state.hidden {
        return scan_block_marks(line, state, false).state;
    }
    let mut rest = line;
    // The two characters before `rest`, nearest last.
    let mut previous = [' ', ' '];
    while let Some(c) = rest.chars().next() {
        let step = match state.literal {
            Literal::Quoted if c == '\\' => {
                1 + rest
                    .get(1..)
                    .and_then(|after| after.chars().next())
                    .map_or(0, char::len_utf8)
            }
            Literal::Quoted => {
                if c == '"' {
                    state.literal = Literal::None;
                }
                c.len_utf8()
            }
            Literal::Raw(hashes) => {
                if c == '"'
                    && rest.as_bytes()[1..]
                        .iter()
                        .take(hashes)
                        .all(|&byte| byte == b'#')
                    && rest.len() > hashes
                {
                    state.literal = Literal::None;
                    1 + hashes
                } else {
                    c.len_utf8()
                }
            }
            Literal::None if state.depth > 0 => {
                if rest.starts_with("/*") {
                    state.depth = state.depth.saturating_add(1);
                    2
                } else if rest.starts_with("*/") {
                    state.depth -= 1;
                    2
                } else {
                    c.len_utf8()
                }
            }
            Literal::None => {
                if rest.starts_with("//") {
                    if holds_disputed_opener(rest) {
                        state.hidden = true;
                        return scan_block_marks(rest, state, false).state;
                    }
                    return state;
                } else if rest.starts_with("/*") {
                    state.depth = 1;
                    2
                } else if c == '"' {
                    state.literal = Literal::Quoted;
                    1
                } else if let Some((hashes, len)) = raw_string_open(rest, previous) {
                    state.literal = Literal::Raw(hashes);
                    len
                } else {
                    char_literal_len(rest).unwrap_or(c.len_utf8())
                }
            }
        };
        let taken = rest.get(..step).unwrap_or(rest);
        let mut last = taken.chars().rev();
        let newest = last.next().unwrap_or(c);
        previous = [last.next().unwrap_or(previous[1]), newest];
        rest = rest.get(step..).unwrap_or("");
    }
    state
}

/// The `#` count and length of the opener (`r#"`) of a raw string that starts
/// `rest`, after `previous`: no identifier ends in the `r`, but a `b` or `c`
/// prefix (`br"…"`, `cr"…"`) belongs to the string.
fn raw_string_open(rest: &str, previous: [char; 2]) -> Option<(usize, usize)> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    if is_ident(previous[1]) && !(matches!(previous[1], 'b' | 'c') && !is_ident(previous[0])) {
        return None;
    }
    let after_r = rest.strip_prefix('r')?;
    let hashes = after_r.bytes().take_while(|&byte| byte == b'#').count();
    after_r
        .get(hashes..)?
        .starts_with('"')
        .then_some((hashes, 1 + hashes + 1))
}

/// What one pass over a line's marks found.
struct LineMarks {
    state: BlockState,
    /// A double-quoted literal was still open at the line's end.
    in_string: bool,
    /// A double-quoted literal held a mark that opens a block comment.
    hidden: bool,
}

/// One left-to-right pass over the marks of `line` in a [`Lexicon::Generic`]
/// grammar, a mark at a time: `**/*.rs` holds the closer `*/`, then `*.rs`,
/// not an opener. With `strings`, outside a block a mark inside a
/// double-quoted literal of the line (`"src/*.rs"`) or a triple-quoted one is
/// text, which [`block_after`] proves nothing by: it notes that a literal hid
/// a mark. Without, every mark counts and no literal is read.
fn scan_block_marks(line: &str, mut state: BlockState, strings: bool) -> LineMarks {
    let mut hidden = false;
    let mut in_string = false;
    if !strings {
        state.triple = false;
    }
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        let step = if in_string {
            hidden |= rest.starts_with("/*");
            if c == '\\' {
                1 + rest
                    .get(1..)
                    .and_then(|after| after.chars().next())
                    .map_or(0, char::len_utf8)
            } else {
                in_string = c != '"';
                c.len_utf8()
            }
        } else if state.depth > 0 {
            if rest.starts_with("/*") {
                state.depth = state.depth.saturating_add(1);
                2
            } else if rest.starts_with("*/") {
                // The `/` is read again: in `*/*` it may open a block, which a
                // block this scan wrongly took as open (one a line comment
                // held) would hide by closing over it.
                state.depth -= 1;
                1
            } else {
                c.len_utf8()
            }
        } else if state.triple {
            hidden |= rest.starts_with("/*");
            if rest.starts_with("\"\"\"") {
                state.triple = false;
                3
            } else {
                c.len_utf8()
            }
        } else if rest.starts_with("/*") {
            state.depth = 1;
            2
        } else if strings && rest.starts_with("\"\"\"") {
            state.triple = true;
            3
        } else {
            in_string = strings && c == '"';
            c.len_utf8()
        };
        rest = rest.get(step..).unwrap_or("");
    }
    LineMarks {
        state,
        in_string,
        hidden,
    }
}

/// The length of the Rust character literal that starts `rest`, as the
/// grammar reads one: a quote, at most one character or escape, a quote. An
/// escape is `\x` and two hex digits, `\u{…}` with hex digits, or `\` and any
/// other one character (`'\''`, `'\\'`). A quote that opens none is a
/// lifetime's (`'a`).
fn char_literal_len(rest: &str) -> Option<usize> {
    let body = rest.strip_prefix('\'')?;
    let hex = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    let content = match body.chars().next()? {
        '\'' => 0,
        '\\' => {
            let escape = body.get(1..)?;
            match escape.chars().next()? {
                'x' => {
                    hex(escape.get(1..3)?).then_some(())?;
                    "\\x".len() + 2
                }
                'u' => {
                    let digits = escape.strip_prefix("u{")?;
                    let close = digits.find('}')?;
                    hex(digits.get(..close)?).then_some(())?;
                    "\\u{".len() + close + "}".len()
                }
                other => 1 + other.len_utf8(),
            }
        }
        other => other.len_utf8(),
    };
    body.get(content..)?
        .starts_with('\'')
        .then_some(1 + content + 1)
}

/// Where a comment may start on `line`: its first comment opener, or the line's
/// start inside a block comment; `usize::MAX` when none can.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "`at` is a `match_indices` hit of ASCII, one byte wide"
    )
)]
fn comment_start(line: &str, block_opened: bool, grammar: &LanguageEntry) -> usize {
    if block_opened {
        return 0;
    }
    let attributes = lexicon(grammar) == Lexicon::Rust;
    COMMENT_OPENERS
        .iter()
        .filter_map(|opener| {
            line.match_indices(opener)
                .find(|(at, _)| {
                    *opener != "#" || !attributes || !line[at + 1..].starts_with(['[', '!'])
                })
                .map(|(at, _)| at)
        })
        .min()
        .unwrap_or(usize::MAX)
}

/// The comment lines ([`CodeSyntax::comment_lines`]) of a text no
/// tree reads, from the places its lines may open a comment: the same
/// lexical judgment the parse gate makes, so a credential in a comment is
/// never left readable because the text failed to parse.
fn lexical_comment_lines(text: &str, grammar: &LanguageEntry, lines: &mut Vec<CommentLine>) {
    let mut block = BlockState::default();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let start = comment_start(body, block.is_open(), grammar);
        block = block_after(body, block, grammar);
        if start < body.len() {
            comment_line_keys(text, line_start + start..line_start + body.len(), lines);
        }
        line_start += line.len();
    }
}

/// Whether `text` may hold a credential in a comment
/// ([`CodeSyntax::comment_lines`]): a line with a key holding a credential
/// word and then `=` or `:`, after a comment opener, or that comes after a block comment
/// opened in the text. A text without one has no credential in a comment, so
/// nothing parses for it.
pub(super) fn has_credential_assignment(text: &str, grammar: &LanguageEntry) -> bool {
    credential_assignment_scan(text, grammar).0
}

/// Whether some line of `text` that holds a quote also holds a key with a
/// credential word in it and then `=` or `:` ([`line_assigns_credential`]):
/// a string literal may hold a config line that no line anchor reaches
/// ([`CodeSyntax::literal_lines`]), after its opening quote or, on a
/// literal's last line, before its closer. Each line is read once.
pub(super) fn has_quoted_credential_assignment(text: &str) -> bool {
    let words: Vec<&str> = crate::security::patterns::CREDENTIAL_WORDS
        .split('|')
        .collect();
    let mut visited = 0;
    text.lines().any(|line| {
        line.contains(['"', '\'', '`']) && line_assigns_credential(line, 0, &words, &mut visited)
    })
}

/// [`has_credential_assignment`] and the bytes its key-character scan looked
/// at, which stays within a bounded multiple of the text's length.
pub(super) fn credential_assignment_scan(text: &str, grammar: &LanguageEntry) -> (bool, usize) {
    let words: Vec<&str> = crate::security::patterns::CREDENTIAL_WORDS
        .split('|')
        .collect();
    let mut visited = 0;
    let mut block = BlockState::default();
    let found = text.lines().any(|line| {
        let lead_end = comment_start(line, block.is_open(), grammar);
        block = block_after(line, block, grammar);
        line_assigns_credential(line, lead_end, &words, &mut visited)
    });
    (found, visited)
}

/// Whether `line` holds, from `from` on, a key with a credential word in it
/// and then `=` or `:`. Each run of key characters is read once, to its end:
/// a run is a key whatever number of credential words it holds, and the
/// operator follows it, past blanks and the key's closing quote.
fn line_assigns_credential(line: &str, from: usize, words: &[&str], visited: &mut usize) -> bool {
    let bytes = line.as_bytes();
    let mut at = from;
    while at < bytes.len() {
        *visited += 1;
        if !is_key_char(char::from(bytes[at])) {
            at += 1;
            continue;
        }
        let run_end = bytes[at..]
            .iter()
            .position(|&byte| !is_key_char(char::from(byte)))
            .map_or(bytes.len(), |len| at + len);
        let run = &bytes[at..run_end];
        *visited += run.len() * words.len();
        let named = words.iter().any(|word| {
            run.windows(word.len())
                .any(|window| window.eq_ignore_ascii_case(word.as_bytes()))
        });
        if named {
            let operator = bytes[run_end..]
                .iter()
                .find(|&&byte| !matches!(byte, b' ' | b'\t' | b'"' | b'\''));
            if operator.is_some_and(|&byte| matches!(byte, b'=' | b':')) {
                return true;
            }
        }
        at = run_end;
    }
    false
}

/// Whether `node` is text rather than code: a string, a comment or a label in
/// the grammar's own node names, or an error or missing node.
fn holds_data(node: Node<'_>) -> bool {
    node.is_error()
        || node.is_missing()
        || (node.is_named()
            && ["string", "comment", "text_block", "template", "label"]
                .iter()
                .any(|word| node.kind().contains(word)))
}

/// Appends the credential-holding lines ([`CodeSyntax::comment_lines`]) of every
/// comment under `node`, in text order.
fn collect_comment_lines(text: &str, root: Node<'_>, lines: &mut Vec<CommentLine>) {
    // A depth-first walk with its own stack: a deeply nested tree must not
    // overflow the call stack.
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind().contains("comment") {
            comment_line_keys(text, node.byte_range(), lines);
        } else {
            let mut cursor = node.walk();
            let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
            pending.extend(children.into_iter().rev());
        }
    }
}

/// What closes a comment on its last line.
const COMMENT_CLOSERS: [&str; 2] = ["*/", "-->"];

/// The characters of a word of a key: the word a tail starts at runs over them.
fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// One comment line a credential can start in
/// ([`CodeSyntax::comment_lines`]).
#[derive(Clone)]
pub(super) struct CommentLine {
    /// The line's text, less the comment's closer.
    pub(super) range: Range<usize>,
    /// Where each key that holds a credential word and that an operator
    /// follows starts, in text order: the key a rule reads for the value
    /// test, from its start to the value. A path or a list position holds
    /// several keys before one operator (`secret/data/app/token:`): its last
    /// is the one listed, the nearest to the value, since a rule reads the
    /// same value from each and the value test reads the end of the key.
    pub(super) keys: Vec<Key>,
}

impl CommentLine {
    /// This line of a source at the positions of the text `edits` made of
    /// it: a key whose start or operator a replacement took is gone, and so
    /// is a line left with none. A key whose value a replacement took reads
    /// on from the placeholder's end, as a read of the rewritten text would.
    pub(super) fn through(&self, edits: &crate::security::Edits) -> Option<Self> {
        let keys: Vec<Key> = self
            .keys
            .iter()
            .filter_map(|key| {
                Some(Key {
                    start: edits.output_at(key.start)?,
                    operator: edits.output_at(key.operator)?,
                    value: edits
                        .output_at(key.value)
                        .unwrap_or(edits.output_end(key.value)),
                })
            })
            .collect();
        (!keys.is_empty()).then(|| Self {
            range: edits.output_end(self.range.start)..edits.output_end(self.range.end),
            keys,
        })
    }
}

/// A key of a comment line ([`CommentLine::keys`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    /// Where the key starts.
    pub(super) start: usize,
    /// Where the operator that ends the key's path is.
    pub(super) operator: usize,
    /// Where the value starts: past the operator and the blanks after it. A
    /// rule's value starts there or at the quote that opens it.
    pub(super) value: usize,
}

/// The characters a key can be made of in any rule that reads a key: a
/// word's own ([`is_key_char`]), a path's `/` and a list position's `[`, `]`.
fn is_key_span_byte(byte: u8) -> bool {
    is_key_char(char::from(byte)) || matches!(byte, b'/' | b'[' | b']')
}

#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are line ends and the closer's ASCII bytes, char boundaries of `comment`"
    )
)]
fn comment_line_keys(text: &str, comment: Range<usize>, lines: &mut Vec<CommentLine>) {
    let mut line_start = comment.start;
    for line in text[comment].split_inclusive(['\n', '\r']) {
        let end = line_start + line.trim_end_matches(['\n', '\r']).len();
        let body = &text[line_start..end];
        // A block's closer is its whole run of stars (`**/`).
        let closed = COMMENT_CLOSERS
            .iter()
            .find_map(|closer| body.trim_end().strip_suffix(closer))
            .map_or(body.len(), |open| open.trim_end_matches('*').len());
        line_keys(&body[..closed], line_start, lines);
        line_start += line.len();
    }
}

/// Appends `body`, one comment line that starts at `offset` in the text, when
/// it holds a key: a rule that reads a key and its value (`key: value`,
/// `KEY=value`) reads the key as one run of key characters (a path's `/` and a
/// list position's `[0]` included) that ends at the operator, past blanks. A
/// key holding a credential word that no operator follows starts no such
/// match, so the line is read once, and each run's operator is looked for once
/// however many credential words the run holds. Each
/// of the credential words of the span starts a key ([`CommentLine::keys`]).
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "offsets are ASCII key bytes and the line's end: char boundaries of `body`"
    )
)]
fn line_keys(body: &str, offset: usize, lines: &mut Vec<CommentLine>) {
    let bytes = body.as_bytes();
    let blank = |byte: &u8| matches!(byte, b' ' | b'\t');
    let mut keys = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if !is_key_span_byte(bytes[at]) {
            at += 1;
            continue;
        }
        let span_end = bytes[at..]
            .iter()
            .position(|&byte| !is_key_span_byte(byte))
            .map_or(bytes.len(), |len| at + len);
        let operator = bytes[span_end..]
            .iter()
            .position(|byte| !blank(byte))
            .map(|blanks| span_end + blanks)
            .filter(|&at| matches!(bytes[at], b'=' | b':'));
        if let Some(operator) = operator {
            let value = offset
                + operator
                + 1
                + bytes[operator + 1..]
                    .iter()
                    .take_while(|b| blank(b))
                    .count();
            let mut run_start = at;
            while run_start < span_end {
                if !is_key_char(char::from(bytes[run_start])) {
                    run_start += 1;
                    continue;
                }
                let run_end = bytes[run_start..span_end]
                    .iter()
                    .position(|&byte| !is_key_char(char::from(byte)))
                    .map_or(span_end, |len| run_start + len);
                if names_a_credential_word(&body[run_start..run_end]) {
                    let key = Key {
                        start: offset + run_start,
                        operator: offset + operator,
                        value,
                    };
                    keys.push(key);
                }
                run_start = run_end;
            }
        }
        at = span_end;
    }
    if !keys.is_empty() {
        lines.push(CommentLine {
            range: offset..offset + bytes.len(),
            keys,
        });
    }
}

/// Whether `word` holds a credential word, in any case.
fn names_a_credential_word(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    crate::security::patterns::CREDENTIAL_WORDS
        .split('|')
        .any(|credential| lower.contains(credential))
}

#[cfg(test)]
mod tests;
