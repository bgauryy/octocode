//! Literals for the nesting pass: strings, template text and regular
//! expressions, skipped as oxc lexes them.

use super::{Kind, Prev, Scan, Step, Stop, is_word_byte};

impl Scan<'_> {
    /// Past a backslash escape, a line continuation included.
    fn escape(&mut self) {
        self.at += if self.rest().starts_with(b"\\\r\n") {
            3
        } else {
            2
        };
    }

    pub(super) fn string(&mut self, quote: u8) -> Step {
        self.count(false)?;
        self.at += 1;
        loop {
            match self.peek(0).ok_or(Stop)? {
                b'\\' => self.escape(),
                b'\n' | b'\r' => return Err(Stop),
                byte => {
                    self.at += 1;
                    if byte == quote {
                        self.prev = Prev::Operand;
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Template text up to its closing backtick or its next `${`.
    pub(super) fn template_text(&mut self) -> Step {
        loop {
            match self.peek(0).ok_or(Stop)? {
                b'\\' => self.escape(),
                b'`' => {
                    self.at += 1;
                    self.prev = Prev::Operand;
                    return Ok(());
                }
                b'$' if self.peek(1) == Some(b'{') => {
                    self.push(Kind::Template)?;
                    self.at += 2;
                    self.prev = Prev::Expression;
                    return Ok(());
                }
                _ => self.at += 1,
            }
        }
    }

    /// A `/` that is not a comment: a regular expression where an
    /// expression may start, else division.
    pub(super) fn regex_or_divide(&mut self) -> Step {
        match self.prev {
            Prev::Unsure => Err(Stop),
            Prev::Operand => self.punct(b'/'),
            Prev::Expression => {
                self.count(false)?;
                self.at += 1;
                let start = self.at;
                self.regex_body()?;
                // Were the body code after all, it nests no deeper than it
                // is long.
                if self.level + (self.at - start) > self.limit {
                    return Err(Stop);
                }
                while self.peek(0).is_some_and(is_word_byte) {
                    self.at += 1;
                }
                self.prev = Prev::Operand;
                Ok(())
            }
        }
    }

    /// A regular expression's body and closing `/`. A body whose groups or
    /// braces do not balance is refused: read as code instead (a `/` that
    /// divides), it could hold a bracket the pass would miss.
    fn regex_body(&mut self) -> Step {
        let (mut class, mut parens, mut braces) = (false, 0_usize, 0_usize);
        loop {
            if self.at_line_end() {
                return Err(Stop);
            }
            let byte = self.peek(0).ok_or(Stop)?;
            self.at += 1;
            match byte {
                b'\\' if self.at_line_end() => return Err(Stop),
                b'\\' => self.at += 1,
                b']' if class => class = false,
                _ if class => {}
                b'[' => class = true,
                b'(' => parens += 1,
                b')' => parens = parens.checked_sub(1).ok_or(Stop)?,
                b'{' => braces += 1,
                b'}' => braces = braces.checked_sub(1).ok_or(Stop)?,
                b'/' if parens == 0 && braces == 0 => return Ok(()),
                b'/' => return Err(Stop),
                _ => {}
            }
        }
    }
}
