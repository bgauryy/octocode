//! Lays out a compact view from a source's comment and literal regions.
//!
//! Every kept line holds text of one source line and carries that line's
//! number, so a citation is right by construction, with nothing to align
//! afterward. Lines are never joined: a newline can be syntax.

use super::apply::KeptLine;
use super::grammars::Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegionKind {
    /// Removed, and it separates what stands on either side of it.
    Comment,
    /// Kept byte for byte.
    Literal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Region {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) kind: RegionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Indent {
    /// Layout is syntax in this language.
    Keep,
    Strip,
}

/// `source` line for line, under each line's own number.
pub(super) fn as_is(source: &str) -> Vec<KeptLine> {
    source
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| KeptLine {
            line: index + 1,
            text: line.to_owned(),
        })
        .collect()
}

/// `source` without blank lines and trailing whitespace: all a reader can do
/// without knowing where a language's strings are. Indentation and spacing
/// inside a line stay, since either can be syntax or content.
pub(super) fn whitespace_only(source: &str) -> Vec<KeptLine> {
    as_is(source)
        .into_iter()
        .filter_map(|mut kept| {
            let ending = if kept.text.ends_with('\n') { "\n" } else { "" };
            let body = kept.text.trim_end_matches([' ', '\t', '\r', '\n']).len();
            (body > 0).then(|| {
                #[cfg_attr(
                    octocode_clippy_ratchet,
                    expect(
                        clippy::disallowed_methods,
                        reason = "The whitespace-only view removes only trailing ASCII whitespace; trim_end_matches supplies a UTF-8 boundary, not an evidence-size cap"
                    )
                )]
                kept.text.truncate(body);
                kept.text.push_str(ending);
                kept
            })
        })
        .collect()
}

/// `source` without its comments and spare whitespace. `regions` are sorted
/// and do not overlap.
pub(super) fn lay_out(
    source: &str,
    regions: &[Region],
    indent: Indent,
    spacing: Spacing,
) -> Vec<KeptLine> {
    let mut writer = Writer::new(indent, spacing);
    let mut pos = 0;
    for region in regions {
        writer.code(&source[pos..region.start]);
        let text = &source[region.start..region.end];
        match region.kind {
            RegionKind::Comment => writer.comment(text),
            RegionKind::Literal => writer.literal(text),
        }
        pos = region.end;
    }
    writer.code(&source[pos..]);
    writer.finish()
}

struct Writer {
    indent: Indent,
    spacing: Spacing,
    kept: Vec<KeptLine>,
    /// 1-based number of the source line being written.
    line: usize,
    text: String,
    /// Whitespace seen on this line and not yet written.
    leading: String,
    at_line_start: bool,
    space_pending: bool,
    /// The last line written ended in a backslash-newline.
    continued: bool,
    /// The last byte written, if it was code (a literal's bytes separate
    /// nothing: only its neighbors decide).
    last_code: Option<u8>,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

impl Writer {
    fn new(indent: Indent, spacing: Spacing) -> Self {
        Self {
            indent,
            spacing,
            kept: Vec::new(),
            line: 1,
            text: String::new(),
            leading: String::new(),
            at_line_start: true,
            space_pending: false,
            continued: false,
            last_code: None,
        }
    }

    fn code(&mut self, code: &str) {
        for piece in code.split_inclusive('\n') {
            match piece.strip_suffix('\n') {
                Some(body) => {
                    self.code_line(body);
                    self.end_line();
                }
                None => self.code_line(piece),
            }
        }
    }

    fn code_line(&mut self, body: &str) {
        // A lone carriage return is a line break to the languages that read
        // one (a JS line terminator): it stays. One ending a CRLF line goes.
        let body = body.strip_suffix('\r').unwrap_or(body);
        let bytes = body.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            let spaces = bytes[start..].iter().take_while(|b| is_space(**b)).count();
            if spaces > 0 {
                if self.at_line_start {
                    if self.indent == Indent::Keep {
                        self.leading.push_str(&body[start..start + spaces]);
                    }
                } else {
                    self.space_pending = true;
                }
                start += spaces;
                continue;
            }
            let word = bytes[start..].iter().take_while(|b| !is_space(**b)).count();
            self.word(&body[start..start + word]);
            start += word;
        }
    }

    fn word(&mut self, word: &str) {
        let first = word.as_bytes().first().copied();
        let tight = first.is_some_and(|byte| self.spacing.before.contains(&byte));
        self.before_content(tight);
        self.text.push_str(word);
        self.last_code = word.as_bytes().last().copied();
    }

    /// A literal's lines stay exact, and a line it runs across has no
    /// indentation of its own to shape.
    fn literal(&mut self, literal: &str) {
        self.before_content(false);
        self.last_code = None;
        for piece in literal.split_inclusive('\n') {
            self.text.push_str(piece);
            if piece.ends_with('\n') {
                self.flush_line();
            } else {
                self.at_line_start = false;
            }
        }
    }

    fn before_content(&mut self, tight: bool) {
        if self.at_line_start {
            self.text.push_str(&self.leading);
            self.leading.clear();
            self.at_line_start = false;
        }
        let after_tight = self
            .last_code
            .is_some_and(|byte| self.spacing.after.contains(&byte));
        if std::mem::take(&mut self.space_pending) && !tight && !after_tight {
            self.text.push(' ');
        }
    }

    /// A comment reads as whitespace: `a/**/b` is two tokens, and one that
    /// spans lines leaves those lines' breaks.
    fn comment(&mut self, comment: &str) {
        if !self.at_line_start {
            self.space_pending = true;
        }
        for _ in comment.bytes().filter(|byte| *byte == b'\n') {
            self.end_line();
        }
    }

    fn end_line(&mut self) {
        // A line after a backslash-newline is part of the one before (a C
        // macro, a Python or shell continuation), even when it is empty.
        if !self.text.is_empty() || self.continued {
            self.text.push('\n');
        }
        self.flush_line();
    }

    fn flush_line(&mut self) {
        self.continued = self.text.ends_with("\\\n") || self.text.ends_with("\\\r\n");
        if !self.text.is_empty() {
            self.kept.push(KeptLine {
                line: self.line,
                text: std::mem::take(&mut self.text),
            });
        }
        self.line += 1;
        self.leading.clear();
        self.at_line_start = true;
        self.space_pending = false;
        self.last_code = None;
    }

    fn finish(mut self) -> Vec<KeptLine> {
        self.flush_line();
        self.kept
    }
}

#[cfg(test)]
mod tests;
