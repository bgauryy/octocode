use grep_regex::RegexMatcherBuilder;

pub struct TextPatternValidationResult {
    pub valid: bool,
    pub error: Option<String>,
}

#[must_use]
pub fn validate_text_pattern(
    pattern: &str,
    fixed_string: bool,
    perl_regex: bool,
) -> TextPatternValidationResult {
    if pattern.is_empty() {
        return TextPatternValidationResult {
            valid: false,
            error: Some("pattern is empty — provide a non-empty search string".to_owned()),
        };
    }

    if fixed_string {
        return TextPatternValidationResult {
            valid: true,
            error: None,
        };
    }

    if perl_regex {
        return match crate::search::text_search::pcre2_builder().build(pattern) {
            Ok(_) => TextPatternValidationResult {
                valid: true,
                error: None,
            },
            Err(err) => TextPatternValidationResult {
                valid: false,
                error: Some(err.to_string()),
            },
        };
    }

    match RegexMatcherBuilder::new().build(pattern) {
        Ok(_) => TextPatternValidationResult {
            valid: true,
            error: None,
        },
        Err(err) => TextPatternValidationResult {
            valid: false,
            error: Some(err.to_string()),
        },
    }
}

/// Whether `pattern` matches a line break, which the line-by-line search
/// (multiline off, the default engine) rejects: it compiles as a regex but
/// not under the `\n` line terminator that search sets.
#[must_use]
pub fn matches_line_break(pattern: &str, fixed_string: bool) -> bool {
    let pattern = if fixed_string {
        regex::escape(pattern)
    } else {
        pattern.to_owned()
    };
    RegexMatcherBuilder::new().build(&pattern).is_ok()
        && RegexMatcherBuilder::new()
            .crlf(true)
            .line_terminator(Some(b'\n'))
            .build(&pattern)
            .is_err()
}

/// Whether `pattern`, read as a regex, means something its writer would
/// write a regex for: an alternation, a class of several characters, a
/// repetition, an anchor, or an escape. A pattern whose regex reading is one
/// fixed text by grouping alone (`pub(crate) fn` reads as `pubcrate fn`,
/// `arr[i]` as `arri`) is code written literally; `false` too for a pattern
/// that does not parse.
#[must_use]
pub fn regex_widens(pattern: &str) -> bool {
    use regex_syntax::hir::{Hir, HirKind};
    fn fixed(hir: &Hir) -> bool {
        match hir.kind() {
            HirKind::Empty | HirKind::Literal(_) => true,
            HirKind::Class(class) => class.literal().is_some(),
            HirKind::Capture(capture) => fixed(&capture.sub),
            HirKind::Concat(parts) => parts.iter().all(fixed),
            HirKind::Look(_) | HirKind::Repetition(_) | HirKind::Alternation(_) => false,
        }
    }
    regex_syntax::parse(pattern).is_ok_and(|hir| pattern.contains('\\') || !fixed(&hir))
}

/// `pattern` with each bracket or repetition operator that pairs with
/// nothing escaped, when only those keep it from parsing as a regex: code
/// written as text (`def route(`, `fn main() {`, `def map[|def imap`) reads
/// with them as themselves and the rest as the regex it is. `None` for a
/// pattern that parses, or that fails for another reason; `(?` opens group
/// syntax, never text.
#[must_use]
pub fn with_unpaired_as_text(pattern: &str) -> Option<String> {
    use regex_syntax::ast::{ErrorKind, parse::Parser};
    let mut read = pattern.to_owned();
    for _ in 0..=pattern.len() {
        let Err(error) = Parser::new().parse(&read) else {
            return (read != pattern).then_some(read);
        };
        let (head, tail) = read.split_at_checked(error.span().start.offset)?;
        let unpaired = match error.kind() {
            ErrorKind::GroupUnclosed => tail.starts_with('(') && !tail.starts_with("(?"),
            ErrorKind::GroupUnopened => tail.starts_with(')'),
            ErrorKind::ClassUnclosed => tail.starts_with('['),
            ErrorKind::RepetitionCountUnclosed | ErrorKind::RepetitionCountDecimalEmpty => {
                tail.starts_with('{')
            }
            ErrorKind::RepetitionMissing => tail.starts_with(['*', '+', '?', '{']),
            _ => false,
        };
        if !unpaired {
            return None;
        }
        read = format!("{head}\\{tail}");
    }
    None
}

#[cfg(test)]
mod tests;
