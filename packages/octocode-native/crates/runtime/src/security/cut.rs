//! Check cuts of source text against what sanitizing the source replaced. A cut
//! taken before sanitizing can split a secret so its prefix or key no longer
//! matches a redaction pattern; it shows part of that secret when its bytes
//! overlap a replaced range ([`exposes`]). Position decides, never text: a
//! one-character cut of a secret is part of it even when that character also
//! occurs elsewhere.

use octocode_engine::security::Edits;
use std::ops::Range;

/// A shown line without the clip markers at its ends: a leading `…`, a
/// trailing `…` or `...` truncation suffix.
pub(crate) fn strip_clip_markers(line: &str) -> &str {
    let line = line.strip_prefix('…').unwrap_or(line);
    let line = line.strip_suffix("...").unwrap_or(line);
    line.strip_suffix('…').unwrap_or(line)
}

/// What a row shows of the lines it was cut from.
pub(crate) enum Cut<'a> {
    /// `text`, starting `offset` bytes into its first line.
    Span { offset: usize, text: &'a str },
    /// This many whole lines, from its first.
    Lines(usize),
}

/// The bytes of `lines` joined by `\n` that `cut`, a row on line `at` (an index
/// of `lines`), shows. A span that is not those lines' bytes at its place is
/// placed on every line it would reach, so it is judged by them whole.
pub(crate) fn placed(lines: &[&str], at: usize, cut: &Cut<'_>) -> Range<usize> {
    let start_of = |line: usize| -> usize { lines[..line].iter().map(|text| text.len() + 1).sum() };
    let whole = |count: usize| {
        let last = (at + count.max(1)).min(lines.len()).max(at + 1) - 1;
        start_of(at)..start_of(last) + lines[last].len()
    };
    match *cut {
        Cut::Lines(count) => whole(count),
        Cut::Span { offset, text } => {
            let mut reached = lines[at..].iter().copied();
            let mut joined = String::new();
            if let Some(first) = reached.next() {
                joined.push_str(first);
            }
            for line in reached.take(text.matches('\n').count()) {
                joined.push('\n');
                joined.push_str(line);
            }
            if joined.get(offset..offset + text.len()) == Some(text) {
                let start = start_of(at) + offset;
                start..start + text.len()
            } else {
                whole(text.lines().count())
            }
        }
    }
}

/// Whether a cut of the source at bytes `cut` shows what sanitizing the source
/// replaced (`redacted`): any of its bytes is one a redaction took.
pub(crate) fn exposes(redacted: &Edits, cut: &Range<usize>) -> bool {
    redacted.overlaps(cut)
}

/// What a row shows of source bytes `at`, its `text`, judged by the scan that
/// decided it exposed: `text` with each run of `runs` it reaches (source ranges,
/// in `at`'s coordinates, with what replaced them) replaced once. A run that
/// reaches past `bound`, the row's own lines, or left nothing, shows as
/// [`REDACTED`]: what replaced it may hold other lines' text. Each byte left
/// shown is checked against `source` (the source's bytes of an unreplaced
/// range), so a row that is not the source's bytes at its place goes whole.
pub(crate) fn rendered<'a, 'b>(
    runs: impl IntoIterator<Item = (Range<usize>, &'a str)>,
    text: &str,
    at: &Range<usize>,
    bound: &Range<usize>,
    source: impl Fn(Range<usize>) -> Option<&'b str>,
) -> String {
    if at.len() != text.len() {
        return REDACTED.to_owned();
    }
    // `text`'s bytes `seg`, when they are the source's at their place.
    let kept = |seg: Range<usize>| {
        let shown = text.get(seg.clone())?;
        (seg.is_empty() || source(at.start + seg.start..at.start + seg.end) == Some(shown))
            .then_some(shown)
    };
    let mut out = String::with_capacity(text.len());
    // Bytes of `text` shown or replaced so far.
    let mut from = 0;
    let mut replaced = false;
    for (range, placeholder) in runs {
        if range.end <= at.start || at.end <= range.start {
            continue;
        }
        // Widened to whole characters: a cut can only over-mask.
        let lo = text.floor_char_boundary(range.start.max(at.start) - at.start);
        let hi = text.ceil_char_boundary(range.end.min(at.end) - at.start);
        if replaced && lo < from {
            from = from.max(hi);
            continue;
        }
        let Some(shown) = kept(from..lo) else {
            return REDACTED.to_owned();
        };
        out.push_str(shown);
        let within = bound.start <= range.start && range.end <= bound.end;
        out.push_str(if within && !placeholder.trim().is_empty() {
            placeholder
        } else {
            REDACTED
        });
        (from, replaced) = (hi, true);
    }
    let Some(shown) = kept(from..text.len()) else {
        return REDACTED.to_owned();
    };
    out.push_str(shown);
    out
}

/// What a span shows for a run whose replacement is unknown or blank.
pub(crate) const REDACTED: &str = "[REDACTED]";

#[cfg(test)]
mod tests {
    use super::*;

    /// A row shows its own bytes with each run it reaches replaced once; a run
    /// past the row's lines, or a row that is not the source's bytes at its
    /// place (whatever its length), goes as the bare placeholder.
    #[test]
    fn a_rendered_row_replaces_exactly_its_runs() {
        let source = format!("{}blob: Qm # ghp_Zq7 tail", " ".repeat(102));
        let at = 102..102 + "blob: Qm # ghp_Z".len();
        let text = &source[at.clone()];
        let line = 0..source.len();
        let from = |r: Range<usize>| source.get(r);
        let render = |runs: &[(Range<usize>, &str)], text: &str, bound: &Range<usize>| {
            rendered(runs.iter().cloned(), text, &at, bound, from)
        };
        assert_eq!(
            render(&[(108..110, "[A]"), (113..120, "[B]")], text, &line),
            "blob: [A] # [B]"
        );
        assert_eq!(
            render(&[(108..112, "")], text, &line),
            "blob: [REDACTED] ghp_Z"
        );
        assert_eq!(render(&[(90..200, "[A]")], text, &(100..150)), REDACTED);
        assert_eq!(render(&[(0..1, "[A]")], text, &line), text);
        let forged = "blob: Qm # xxx_Z";
        assert_eq!(render(&[(108..110, "[A]")], forged, &line), REDACTED);
        assert_eq!(
            rendered([(108..110, "[A]")], text, &(0..3), &line, from),
            REDACTED
        );
        let unicode = "\u{fc}\u{b7}\u{e9}";
        assert_eq!(
            rendered(
                [(1..2, "[A]")],
                unicode,
                &(0..unicode.len()),
                &(0..9),
                |r| unicode.get(r)
            ),
            "[A]\u{b7}\u{e9}"
        );
    }

    const SECRET: &str = "ghp_Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7";

    fn redacted(source: &str) -> Edits {
        let sanitized = octocode_engine::security::sanitizer::sanitize_content(source, None)
            .expect("sanitized");
        assert!(sanitized.has_secrets, "{source}");
        sanitized.edits
    }

    fn span(line: &str, offset: usize, len: usize) -> Range<usize> {
        placed(
            &[line],
            0,
            &Cut::Span {
                offset,
                text: &line[offset..offset + len],
            },
        )
    }

    /// A one-character cut inside a secret shows part of it, though the same
    /// character occurs outside the secret on the same line.
    #[test]
    fn a_one_character_cut_of_a_secret_is_exposed_wherever_its_text_recurs() {
        let line = format!("Zq7 g h p _ let token = \"{SECRET}\"; Zq7");
        let edits = redacted(&line);
        let start = line.find(SECRET).expect("secret");
        for at in start..start + SECRET.len() {
            assert!(line[..start].contains(&line[at..=at]));
            assert!(exposes(&edits, &span(&line, at, 1)), "byte {at}");
        }
        // Three characters that recur outside the secret, cut from inside it.
        assert!(line[..start].contains(&line[start + 4..start + 7]));
        assert!(exposes(&edits, &span(&line, start + 4, 3)));
    }

    /// A cut wholly outside every redaction shows none.
    #[test]
    fn a_cut_outside_every_redaction_is_not_exposed() {
        let line = format!("let token = \"{SECRET}\"; let next = 1;");
        let edits = redacted(&line);
        let start = line.find(SECRET).expect("secret");
        assert!(!exposes(&edits, &span(&line, 0, 3)));
        assert!(!exposes(&edits, &span(&line, 0, start)));
        let after = start + SECRET.len();
        assert!(!exposes(&edits, &span(&line, after, line.len() - after)));
    }

    /// A cut that straddles a redaction's edge shows its part of the secret.
    #[test]
    fn a_cut_across_a_redaction_edge_is_exposed() {
        let line = format!("let token = \"{SECRET}\";");
        let edits = redacted(&line);
        let start = line.find(SECRET).expect("secret");
        let end = start + SECRET.len();
        assert!(exposes(&edits, &span(&line, start - 2, 3)));
        assert!(exposes(&edits, &span(&line, end - 1, 2)));
        assert!(!exposes(&edits, &span(&line, end, 2)));
        assert!(exposes(&edits, &span(&line, 0, line.len())));
    }

    /// Positions are bytes of multi-byte text: a cut of a character beside a
    /// secret stays shown, one of the secret's own does not.
    #[test]
    fn unicode_cuts_are_judged_by_byte_position() {
        let line = format!("éé token = \"{SECRET}\" · ππ Zq7");
        let edits = redacted(&line);
        let start = line.find(SECRET).expect("secret");
        for (at, c) in line.char_indices() {
            let inside = (start..start + SECRET.len()).contains(&at);
            assert_eq!(
                exposes(&edits, &span(&line, at, c.len_utf8())),
                inside,
                "{c:?} at {at}"
            );
        }
    }

    /// A row is placed on its line of a window; a span that is not there
    /// takes its lines whole, and a lines row its lines.
    #[test]
    fn rows_are_placed_in_their_window() {
        let lines = ["ab", "céd", "ef"];
        let cut = Cut::Span {
            offset: 1,
            text: "é",
        };
        assert_eq!(placed(&lines, 1, &cut), 4..6);
        let cut = Cut::Span {
            offset: 1,
            text: "éd\nef",
        };
        assert_eq!(placed(&lines, 1, &cut), 4..10);
        let cut = Cut::Span {
            offset: 0,
            text: "zz",
        };
        assert_eq!(placed(&lines, 1, &cut), 3..7);
        assert_eq!(placed(&lines, 0, &Cut::Lines(2)), 0..7);
        assert_eq!(placed(&lines, 2, &Cut::Lines(5)), 8..10);
    }
}
