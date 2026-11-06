//! An upper bound on how deep a JS/TS file nests, from one pass over its
//! bytes, so most files never pay for a syntax tree before oxc reads them.
//!
//! The pass lexes what decides bracket structure the way oxc does: strings,
//! template literals, comments, regular expressions and JSX. Every bracket
//! group is a level, and every token since the last statement boundary in a
//! group is one more (a `1+1+…` or `a.b.…` chain, unary prefixes, `else if`),
//! so the bound is at least the depth of the syntax tree. Where oxc's reading
//! could differ from the pass's (a `/` or `<` after `}`, a contextual keyword,
//! a postfix `!`, `++` or `--`, a generic arrow in TSX) the pass is unsure and
//! the caller measures the tree instead.

mod jsx;
mod literals;

/// What came before the current byte, as far as a following `/` or `<`
/// depends on it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    /// An operand ended: `/` divides, `<` compares.
    Operand,
    /// An expression may start: `/` opens a regular expression, `<` JSX.
    Expression,
    /// oxc's reading depends on context the pass does not track.
    Unsure,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Root,
    /// `control`: opened right after `if`, `while`, `for`, `with` (or
    /// `for await`), so what follows its `)` starts a statement.
    Paren {
        control: bool,
    },
    Bracket,
    Brace,
    /// `${` of a template literal.
    Template,
    /// `{` of a JSX attribute (`children: false`) or child.
    JsxExpr {
        children: bool,
    },
    /// A JSX element: its attributes, then (`children`) its children.
    Jsx {
        children: bool,
    },
}

struct Frame {
    kind: Kind,
    /// Tokens of this group since its last statement boundary.
    run: usize,
    /// A `;` ended a statement: the next token starts a new one, unless it
    /// continues the statement (`else`, `while` of `do … while`).
    boundary: bool,
}

struct Scan<'a> {
    bytes: &'a [u8],
    at: usize,
    jsx: bool,
    limit: usize,
    frames: Vec<Frame>,
    /// Σ (1 + run) over `frames`: the bound at the current byte.
    level: usize,
    prev: Prev,
    /// The last token was a word naming a statement whose `(…)` is followed
    /// by a statement; `for_word`: it was `for` (`for await (`).
    control: bool,
    for_word: bool,
    after_dot: bool,
}

/// Syntax-tree levels the pass counts no token for, over or under its
/// deepest token: every bracket group and every operator, keyword and
/// literal spells one, except the node of a statement that is only an
/// expression (`expression_statement`) and the inner node of a literal the
/// pass counts as one token (`string_fragment`, `regex_pattern`,
/// `jsx_text`). `examples/nesting_sweep.rs` checks the tree is never deeper
/// than the bound over a corpus.
const LEAF_LEVELS: usize = 2;

/// The pass stopped early: the bound passed its limit, or the pass cannot
/// vouch for its reading.
struct Stop;

type Step = Result<(), Stop>;

/// The bound, when the pass reads `content` to the end without passing
/// `limit` and vouches for every token; `None` sends the caller to measure.
/// `jsx`: the dialect reads JSX (JavaScript and TSX).
pub(super) fn nesting_bound(content: &str, jsx: bool, limit: usize) -> Option<usize> {
    let mut scan = Scan {
        bytes: content.as_bytes(),
        at: 0,
        jsx,
        limit,
        frames: vec![Frame {
            kind: Kind::Root,
            run: 0,
            boundary: false,
        }],
        level: 1,
        prev: Prev::Expression,
        control: false,
        for_word: false,
        after_dot: false,
    };
    if scan.bytes.starts_with(b"#!") {
        scan.skip_line();
    }
    let mut deepest = scan.level;
    while scan.at < scan.bytes.len() {
        let step = match scan.frames.last().map(|frame| frame.kind) {
            Some(Kind::Jsx { children }) => scan.jsx_step(children),
            _ => scan.code_step(),
        };
        if step.is_err() {
            return None;
        }
        deepest = deepest.max(scan.level);
    }
    Some(deepest + LEAF_LEVELS).filter(|&bound| bound <= limit)
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'\\') || byte >= 0x80
}

impl Scan<'_> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.at + offset).copied()
    }

    fn rest(&self) -> &[u8] {
        self.bytes.get(self.at..).unwrap_or_default()
    }

    fn within_limit(&self) -> Step {
        if self.level > self.limit {
            Err(Stop)
        } else {
            Ok(())
        }
    }

    /// One token of the innermost group; a token after a `;` starts its
    /// statement over unless it `continues` the statement before.
    fn count(&mut self, continues: bool) -> Step {
        let frame = self.frames.last_mut().ok_or(Stop)?;
        if std::mem::take(&mut frame.boundary) && !continues {
            self.level -= frame.run;
            frame.run = 0;
        }
        frame.run += 1;
        self.level += 1;
        self.within_limit()
    }

    fn push(&mut self, kind: Kind) -> Step {
        self.count(false)?;
        self.frames.push(Frame {
            kind,
            run: 0,
            boundary: false,
        });
        self.level += 1;
        self.within_limit()
    }

    fn pop(&mut self) -> Result<Kind, Stop> {
        if self.frames.len() == 1 {
            return Err(Stop);
        }
        let frame = self.frames.pop().ok_or(Stop)?;
        self.level -= 1 + frame.run;
        Ok(frame.kind)
    }

    /// Whether a JS line terminator starts at the current byte: LF, CR, or
    /// U+2028 or U+2029.
    fn at_line_end(&self) -> bool {
        matches!(self.peek(0), Some(b'\n' | b'\r'))
            || self.rest().starts_with("\u{2028}".as_bytes())
            || self.rest().starts_with("\u{2029}".as_bytes())
    }

    /// Past a `//` comment (or a hashbang), to its line terminator.
    fn skip_line(&mut self) {
        while self.at < self.bytes.len() && !self.at_line_end() {
            self.at += 1;
        }
    }

    /// One token in code, or the space or comment before one.
    fn code_step(&mut self) -> Step {
        let Some(byte) = self.peek(0) else {
            return Ok(());
        };
        match byte {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => {
                self.at += 1;
                return Ok(());
            }
            b'/' if self.peek(1) == Some(b'/') => {
                self.skip_line();
                return Ok(());
            }
            b'/' if self.peek(1) == Some(b'*') => return self.block_comment(),
            _ => {}
        }
        let control = std::mem::take(&mut self.control);
        let for_word = std::mem::take(&mut self.for_word);
        let after_dot = std::mem::take(&mut self.after_dot);
        match byte {
            b'/' => self.regex_or_divide(),
            b'"' | b'\'' => self.string(byte),
            b'`' => {
                self.count(false)?;
                self.at += 1;
                self.template_text()
            }
            b'(' => self.open(Kind::Paren { control }),
            b'[' => self.open(Kind::Bracket),
            b'{' => self.open(Kind::Brace),
            b')' | b']' | b'}' => self.close(byte),
            // HTML-like comments, which oxc reads in scripts only.
            b'<' if self.rest().starts_with(b"<!--") => Err(Stop),
            b'-' if self.rest().starts_with(b"-->") => Err(Stop),
            b'<' if self.jsx && self.prev != Prev::Operand => {
                if self.prev == Prev::Unsure {
                    return Err(Stop);
                }
                self.count(false)?;
                self.at += 1;
                self.jsx_open()
            }
            b';' => {
                self.count(false)?;
                self.at += 1;
                self.frames.last_mut().ok_or(Stop)?.boundary = true;
                self.prev = Prev::Expression;
                Ok(())
            }
            _ if is_word_byte(byte) => self.word(after_dot, for_word),
            _ => self.punct(byte),
        }
    }

    fn block_comment(&mut self) -> Step {
        let rest = self.bytes.get(self.at + 2..).unwrap_or_default();
        let end = rest.windows(2).position(|pair| pair == b"*/").ok_or(Stop)?;
        self.at += 2 + end + 2;
        Ok(())
    }

    fn word(&mut self, after_dot: bool, for_word: bool) -> Step {
        let start = self.at;
        while self.peek(0).is_some_and(is_word_byte) {
            self.at += 1;
        }
        let word = self.bytes.get(start..self.at).unwrap_or_default();
        self.count(matches!(word, b"else" | b"while"))?;
        if after_dot {
            self.prev = Prev::Operand;
            return Ok(());
        }
        self.control =
            matches!(word, b"if" | b"while" | b"for" | b"with") || (for_word && word == b"await");
        self.for_word = word == b"for";
        self.prev = match word {
            b"return" | b"typeof" | b"instanceof" | b"in" | b"new" | b"delete" | b"void"
            | b"throw" | b"case" | b"do" | b"else" | b"extends" | b"default" => Prev::Expression,
            b"of" | b"yield" | b"await" => Prev::Unsure,
            _ => Prev::Operand,
        };
        Ok(())
    }

    fn punct(&mut self, byte: u8) -> Step {
        self.count(false)?;
        self.at += 1;
        self.after_dot = byte == b'.';
        self.prev = match byte {
            b'+' | b'-' if self.peek(0) == Some(byte) => {
                self.at += 1;
                Prev::Unsure
            }
            // A TypeScript non-null assertion (`x!`), or a `!` that starts
            // a statement after a line break.
            b'!' if self.prev == Prev::Operand && self.peek(0) != Some(b'=') => Prev::Unsure,
            _ => Prev::Expression,
        };
        Ok(())
    }

    fn open(&mut self, kind: Kind) -> Step {
        self.push(kind)?;
        self.at += 1;
        self.prev = Prev::Expression;
        Ok(())
    }

    fn close(&mut self, byte: u8) -> Step {
        let kind = self.pop()?;
        self.at += 1;
        self.prev = match (byte, kind) {
            (b')', Kind::Paren { control: true }) => Prev::Expression,
            (b')', Kind::Paren { control: false }) | (b']', Kind::Bracket) => Prev::Operand,
            (b'}', Kind::Brace) => Prev::Unsure,
            (b'}', Kind::Template) => return self.template_text(),
            (b'}', Kind::JsxExpr { .. }) => Prev::Operand,
            _ => return Err(Stop),
        };
        Ok(())
    }
}
