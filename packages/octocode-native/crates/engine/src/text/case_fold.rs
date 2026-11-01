//! Case-insensitive matching over a source lowercased once, with every
//! offset into the lowercase copy mapped back to the source's own bytes.
//!
//! Lowercasing changes byte lengths per char (`İ` → `i̇` gains a byte, `ẞ` →
//! `ß` loses one), even where the whole text keeps its length, so an offset
//! into `text.to_lowercase()` never slices `text` directly. `str::to_lowercase`
//! folds per char except for a final `Σ`, whose two forms have the same
//! length, so the per-char lengths are the whole map.
use std::ops::Range;

/// `source` lowercased once, with the map back: each char whose lowercase
/// form has another byte length, as (its lowercase bytes, its source bytes),
/// in order. Ordinary text keeps the map empty.
pub struct Folded<'a> {
    source: &'a str,
    text: String,
    changes: Vec<(Range<usize>, Range<usize>)>,
}

impl<'a> Folded<'a> {
    #[must_use]
    pub fn of(source: &'a str) -> Self {
        let mut changes = Vec::new();
        let mut folded_start = 0;
        for (start, character) in source.char_indices() {
            let width = character.len_utf8();
            let folded_end =
                folded_start + character.to_lowercase().map(char::len_utf8).sum::<usize>();
            if folded_end - folded_start != width {
                changes.push((folded_start..folded_end, start..start + width));
            }
            folded_start = folded_end;
        }
        Self {
            source,
            text: source.to_lowercase(),
            changes,
        }
    }

    /// The lowercase copy.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The source bytes of the char whose lowercase form holds byte `at` of
    /// [`Self::text`]; past the end, the empty range at the source's end.
    #[must_use]
    pub fn origin(&self, at: usize) -> Range<usize> {
        if at >= self.text.len() {
            return self.source.len()..self.source.len();
        }
        let index = self.changes.partition_point(|(folded, _)| folded.end <= at);
        if let Some((folded, source)) = self.changes.get(index)
            && folded.start <= at
        {
            return source.clone();
        }
        // Between changed chars every char keeps its width: the offset
        // shifts by the widths the changes before it gained or lost.
        let byte = self.source.floor_char_boundary(
            match index.checked_sub(1).map(|before| &self.changes[before]) {
                Some((folded, source)) => at - folded.end + source.end,
                None => at,
            },
        );
        let width = self.source[byte..].chars().next().map_or(0, char::len_utf8);
        byte..byte + width
    }

    /// The source span a match at `folded` (a byte range of [`Self::text`])
    /// covers: from its first char's start to its last char's end, whole
    /// chars only.
    #[must_use]
    pub fn span(&self, folded: Range<usize>) -> Range<usize> {
        let start = self.origin(folded.start).start;
        let end = if folded.end > folded.start {
            self.origin(folded.end - 1).end
        } else {
            start
        };
        start..end.max(start)
    }

    /// The source char index of the char [`Self::origin`] names; the tests'
    /// oracle for the byte mapping.
    #[cfg(test)]
    #[must_use]
    pub fn char_index(&self, at: usize) -> usize {
        self.source[..self.origin(at).start].chars().count()
    }

    /// Spans of `pattern`, ignoring case, in source byte coordinates.
    #[must_use]
    pub fn ranges(&self, pattern: &str) -> Vec<Range<usize>> {
        let needle = pattern.to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.text
            .match_indices(&needle)
            .map(|(at, matched)| self.span(at..at + matched.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: [&str; 7] = [
        "const s = \"İéxẞ\";",
        "DİYARBAKIR İstanbul ı I",
        "STRAẞE straße ẞß",
        "e\u{301}cole Ё\u{308} a\u{30a}\u{301}",
        "👍🏽 👩‍👩‍👧 ΟΔΟΣ Σ σ",
        "ﬁle ǅ Ω K Å",
        "+ let x = \"🎉 İ\"; // ẞ",
    ];

    /// Every char's lowercase byte offset maps back to that char, whatever
    /// lowercasing does to the line's bytes: `İ` grows, `ẞ` shrinks (and
    /// together keep the line's length), combining marks, emoji and a
    /// final sigma keep theirs.
    #[test]
    fn lowercase_offsets_map_back_to_their_chars_in_any_script() {
        for line in LINES {
            let folded = Folded::of(line);
            let lower = folded.text();
            let mut at = 0;
            for (index, (start, c)) in line.char_indices().enumerate() {
                assert!(lower.is_char_boundary(at), "{line:?} at {at}");
                assert_eq!(folded.char_index(at), index, "{line:?} at {at}");
                assert_eq!(folded.origin(at), start..start + c.len_utf8(), "{line:?}");
                // An offset inside the char's lowercase form is that char's.
                let width: usize = c.to_lowercase().map(char::len_utf8).sum();
                assert_eq!(folded.char_index(at + width - 1), index, "{line:?}");
                at += width;
            }
            assert_eq!(at, lower.len(), "{line:?}");
            assert_eq!(folded.char_index(at), line.chars().count());
            assert_eq!(folded.origin(at), line.len()..line.len());
        }
    }

    /// A match found in the lowercase text covers exactly the original
    /// chars it lowercased from, wherever `İ`/`ẞ` shift the bytes.
    #[test]
    fn a_lowercase_match_spans_its_original_chars() {
        for (line, needle, original) in [
            ("İéxẞ needle İ", "needle", "needle"),
            ("İéxẞ", "i\u{307}éxß", "İéxẞ"),
            ("DİYARBAKIR", "di\u{307}yarbakir", "DİYARBAKIR"),
            ("ΟΔΟΣ x", "οδος", "ΟΔΟΣ"),
            ("STRAẞE", "straße", "STRAẞE"),
            ("👍🏽 ÉCOLE", "école", "ÉCOLE"),
        ] {
            let folded = Folded::of(line);
            let at = folded.text().find(needle).expect(needle);
            assert_eq!(
                &line[folded.span(at..at + needle.len())],
                original,
                "{line:?}"
            );
        }
    }

    /// One folded copy serves every case-insensitive pattern, and each span
    /// slices the source's own bytes, around length-changing chars too.
    #[test]
    fn one_fold_serves_every_pattern_in_source_coordinates() {
        let source = "İneedle and İNEEDLE, then Straße STRASSE";
        let folded = Folded::of(source);
        let text = |spans: Vec<Range<usize>>| {
            spans
                .into_iter()
                .map(|span| &source[span])
                .collect::<Vec<_>>()
        };
        assert_eq!(text(folded.ranges("needle")), ["needle", "NEEDLE"]);
        assert_eq!(text(folded.ranges("İ")), ["İ", "İ"]);
        assert_eq!(text(folded.ranges("straße")), ["Straße"]);
        assert_eq!(text(folded.ranges("THEN")), ["then"]);
        assert!(folded.ranges("").is_empty());
    }
}
