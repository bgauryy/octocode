//! The byte ranges a judge wrote into the text a file is searched as
//! ([`crate::types::ServedText::written`]), and the one rule every count of a
//! served file follows: a match the placeholder's own spelling makes is no
//! match ([`each_own`]).
use std::ops::Range;

/// Whether `found` (a match's byte range) takes a byte of `written`
/// (ascending, disjoint ranges of the same text). An empty match takes one
/// when it sits strictly inside a written range: between two of its bytes.
pub(super) fn takes(written: &[Range<usize>], found: Range<usize>) -> bool {
    // The first range that ends after the match starts.
    let first = written.partition_point(|range| range.end <= found.start);
    written.get(first).is_some_and(|range| {
        if found.is_empty() {
            range.start < found.start
        } else {
            range.start < found.end
        }
    })
}

/// The written ranges that meet `block` (a range of the searched text), as
/// ranges of the block's own bytes: what a matcher run over the block alone
/// must not take.
pub(super) fn within(written: &[Range<usize>], block: Range<usize>) -> Vec<Range<usize>> {
    let first = written.partition_point(|range| range.end <= block.start);
    written[first..]
        .iter()
        .take_while(|range| range.start < block.end)
        .map(|range| {
            range.start.max(block.start) - block.start..range.end.min(block.end) - block.start
        })
        .collect()
}

/// Each match of `matcher` in `bytes` that the source's text makes, left to
/// right and non-overlapping, as `find_iter` yields them (`matched` returns
/// whether to go on); `hidden` are the ranges of `bytes` a judge wrote. A
/// match that takes none of them is the source's own text. One that takes
/// some counts only when it holds whatever was written there: it matches the
/// same bytes with every written byte made [`FILLER`], a byte no placeholder
/// spells, so `.*` or `\S+` across a masked value still matches, as the
/// source does, while a match the placeholder's own spelling makes
/// (`REDACTED`, `\[`, `[A-Z]{5}`) is none. After one that is none, the search
/// goes on from the character after where it began, so a shorter or later
/// match it covered is still found (`\]?"` finds the `"` after a
/// placeholder's `]`). Neither text holds a byte the judge withheld.
pub(super) fn each_own<M: grep_matcher::Matcher>(
    matcher: &M,
    bytes: &[u8],
    hidden: &[Range<usize>],
    mut matched: impl FnMut(grep_matcher::Match) -> bool,
) -> Result<(), M::Error> {
    if hidden.is_empty() {
        return matcher.find_iter(bytes, matched);
    }
    let mut filled: Option<Vec<u8>> = None;
    let (mut at, mut last_end) = (0, None);
    while at <= bytes.len() {
        let Some(found) = matcher.find_at(bytes, at)? else {
            break;
        };
        if takes(hidden, found.start()..found.end()) {
            let filled = filled.get_or_insert_with(|| fill(bytes, hidden));
            let holds = matcher
                .find_at(filled, found.start())?
                .is_some_and(|again| again == found);
            if !holds {
                at = next_char(bytes, found.start());
                continue;
            }
        }
        // As `find_iter`: an empty match steps one byte on, and one right
        // where the last match ended is none.
        if found.is_empty() {
            at = found.end() + 1;
            if last_end == Some(found.end()) {
                continue;
            }
        } else {
            at = found.end();
        }
        last_end = Some(found.end());
        if !matched(found) {
            break;
        }
    }
    Ok(())
}

/// What stands in for every written byte when a match is tried again
/// ([`each_own`]): a control byte, which no placeholder or blank a judge
/// writes holds, so a match that needs a placeholder's characters fails on
/// it, and one byte for one, so every offset stays.
const FILLER: u8 = 0x01;

/// `bytes` with each byte of `hidden` made [`FILLER`].
fn fill(bytes: &[u8], hidden: &[Range<usize>]) -> Vec<u8> {
    let mut filled = bytes.to_vec();
    for range in hidden {
        if let Some(written) = filled.get_mut(range.clone()) {
            written.fill(FILLER);
        }
    }
    filled
}

/// The first byte of the character after the one at `at` in `bytes`.
fn next_char(bytes: &[u8], at: usize) -> usize {
    let mut next = at + 1;
    while bytes.get(next).is_some_and(|byte| byte & 0xC0 == 0x80) {
        next += 1;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_takes_a_written_byte_only_when_it_overlaps_one() {
        let written = [4..8, 12..13];
        // Beside a written range on either side: the source's own bytes.
        assert!(!takes(&written, 0..4));
        assert!(!takes(&written, 8..12));
        assert!(!takes(&written, 13..20));
        // Any overlap takes it.
        assert!(takes(&written, 3..5));
        assert!(takes(&written, 7..9));
        assert!(takes(&written, 5..6));
        assert!(takes(&written, 0..20));
        assert!(takes(&written, 12..13));
        // An empty match at a written range's edge is between source bytes
        // and written ones; strictly inside, it is the written text's.
        assert!(!takes(&written, 4..4));
        assert!(!takes(&written, 8..8));
        assert!(takes(&written, 6..6));
        assert!(!takes(&[], 0..10));
    }

    /// Every own match, none taking a written byte, and a later match a
    /// refused one covered is still found.
    #[test]
    fn each_own_match_skips_only_matches_that_take_written_bytes() {
        let matcher = grep_regex::RegexMatcher::new(r#"\]?";"#).expect("pattern");
        let bytes = br#"a = "[HID]"; b = "x]";"#;
        let hidden = std::iter::once(5..10).collect::<Vec<_>>();
        let mut found = Vec::new();
        each_own(&matcher, bytes, &hidden, |m| {
            found.push((m.start(), m.end()));
            true
        })
        .expect("search");
        // `]";` at 9 takes the written `]`; its `";` at 10 is the source's.
        assert_eq!(found, vec![(10, 12), (19, 22)]);
        for spelled in ["HID", r"\[", "[A-Z]{3}", r#"= "\["#] {
            let matcher = grep_regex::RegexMatcher::new(spelled).expect("pattern");
            let mut none = 0;
            each_own(&matcher, bytes, &hidden, |_| {
                none += 1;
                true
            })
            .expect("search");
            assert_eq!(none, 0, "{spelled}");
        }
        // A match that holds whatever was written counts, whole.
        for any in [r#"= ".*"; b"#, r"\S+;", ".{6}"] {
            let matcher = grep_regex::RegexMatcher::new(any).expect("pattern");
            let ours = find_all(&matcher, bytes, &hidden);
            let theirs = find_all(&matcher, bytes, &[]);
            assert_eq!(ours, theirs, "{any}");
        }
    }

    fn find_all(
        matcher: &grep_regex::RegexMatcher,
        bytes: &[u8],
        hidden: &[Range<usize>],
    ) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        each_own(matcher, bytes, hidden, |m| {
            found.push((m.start(), m.end()));
            true
        })
        .expect("search");
        found
    }

    #[test]
    fn the_written_ranges_of_a_block_are_its_own_offsets() {
        let written = [2..6, 10..14, 30..31];
        assert_eq!(within(&written, 4..12), vec![0..2, 6..8]);
        assert_eq!(within(&written, 6..10), Vec::<Range<usize>>::new());
        assert_eq!(within(&written, 0..40), vec![2..6, 10..14, 30..31]);
    }
}
