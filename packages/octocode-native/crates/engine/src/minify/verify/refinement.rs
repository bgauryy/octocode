//! Compare lexical token evidence across parser regrouping. Preprocessor
//! replacements aggregate tokens; their grouping can change across empty
//! definitions without changing preprocessing. Interior bytes stay exact.

#[derive(Clone, Copy, Debug)]
pub(super) struct Token<'a> {
    pub(super) text: &'a str,
    /// A preprocessor replacement contains several lexical tokens. Grammar
    /// scanners can move their boundaries across adjacent empty definitions.
    pub(super) aggregate: bool,
}

pub(super) fn refines(
    source: &str,
    before: &[Token<'_>],
    view: &str,
    after: &[Token<'_>],
) -> Result<(), String> {
    let squash = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let offset = |text: &str, token: &str| token.as_ptr() as usize - text.as_ptr() as usize;
    let span = |text: &str, tokens: &[Token<'_>], start: usize, end: usize| {
        let first = offset(text, tokens[start].text);
        let last = offset(text, tokens[end - 1].text) + tokens[end - 1].text.len();
        first..last
    };
    let group = |tokens: &[Token<'_>], start: usize, wanted: usize| {
        let mut end = start;
        let mut bytes = 0;
        while end < tokens.len() && (end == start || bytes < wanted) {
            bytes += squash(tokens[end].text).len();
            end += 1;
        }
        end
    };
    let (mut left, mut right) = (0, 0);
    while left < before.len() {
        let Some(next) = after.get(right) else {
            return Err(format!(
                "token {left} {:?} is missing from the view",
                before[left].text
            ));
        };
        let reverse = next.aggregate && squash(next.text).len() > squash(before[left].text).len();
        let (left_end, right_end) = if reverse {
            (group(before, left, squash(next.text).len()), right + 1)
        } else {
            (
                left + 1,
                group(after, right, squash(before[left].text).len()),
            )
        };
        let original = &source[span(source, before, left, left_end)];
        let compact = &view[span(view, after, right, right_end)];
        let aggregate = before[left..left_end]
            .iter()
            .chain(&after[right..right_end])
            .any(|token| token.aggregate);
        let same = if aggregate {
            original.trim_end_matches([' ', '\t', '\r'])
                == compact.trim_end_matches([' ', '\t', '\r'])
        } else {
            original == compact || (squash(original).is_empty() && squash(compact).is_empty())
        };
        if !same {
            return Err(format!("token {left} changed: {original:?} -> {compact:?}"));
        }
        left = left_end;
        right = right_end;
    }
    match after.get(right) {
        Some(extra) => Err(format!("the view adds token {right} {extra:?}")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Token, refines};

    fn tokens<'a>(text: &'a str, cuts: &[(usize, usize)]) -> Vec<Token<'a>> {
        cuts.iter()
            .map(|&(start, end)| Token {
                text: &text[start..end],
                aggregate: false,
            })
            .collect()
    }

    #[test]
    fn preprocessor_aggregation_never_hides_changed_literal_bytes() {
        let source = "#define Message \"a  b\" \r";
        for view in ["#define Message \"a  b\"", "#define Message \"a b\""] {
            let before = [Token {
                text: source,
                aggregate: true,
            }];
            let after = [Token {
                text: view,
                aggregate: true,
            }];
            assert_eq!(
                refines(source, &before, view, &after).is_ok(),
                view.contains("a  b")
            );
        }
    }

    #[test]
    fn a_split_of_the_same_bytes_passes_and_a_glue_fails() {
        // `>>` kept as `>>`, read as one token before and two after.
        let (source, view) = ("a>>b", "a>>b");
        let before = tokens(source, &[(0, 1), (1, 3), (3, 4)]);
        let after = tokens(view, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        assert!(refines(source, &before, view, &after).is_ok());
        // `> >` emitted as `>>`: two tokens glued into one.
        let (source, view) = ("a> >b", "a>>b");
        let before = tokens(source, &[(0, 1), (1, 2), (3, 4), (4, 5)]);
        let after = tokens(view, &[(0, 1), (1, 3), (3, 4)]);
        assert!(refines(source, &before, view, &after).is_err());
        // A split whose bytes changed (`> >` read as one token, emitted `>>`
        // read as two) is a change too.
        let (source, view) = ("a> >b", "a>>b");
        let before = tokens(source, &[(0, 1), (1, 4), (4, 5)]);
        let after = tokens(view, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        assert!(refines(source, &before, view, &after).is_err());
    }
}
