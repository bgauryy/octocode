use super::*;
use crate::tools::local_search::rows::owned_lines;

/// A small deterministic generator (xorshift), so a failure reproduces.
struct Draw(u64);

impl Draw {
    fn below(&mut self, bound: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        u32::try_from(self.0 % u64::from(bound.max(1))).expect("small")
    }
}

fn row(line: u32) -> TextSearchMatch {
    TextSearchMatch {
        line,
        first_line: line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        deferred: None,
        value: String::new(),
        value_digest: [0; 32],
        count: None,
        rank: None,
    }
}

/// The lines a file's later rows name: the first `max` distinct lines of
/// `rows[end..]`, and how many distinct lines follow.
fn later_by_definition(rows: &[TextSearchMatch], end: usize) -> (Vec<u32>, Option<u32>) {
    let mut distinct = rows[end..].iter().map(|row| row.line).collect::<Vec<_>>();
    distinct.sort_unstable();
    distinct.dedup();
    let unlisted = distinct.len().saturating_sub(MAX_MORE_LINES);
    distinct.truncate(MAX_MORE_LINES);
    (distinct, (unlisted > 0).then(|| saturating_u32(unlisted)))
}

#[test]
fn later_lines_list_the_first_lines_and_count_the_rest_at_every_end() {
    let rows = [717, 711, 712, 711, 802].map(row);
    assert_eq!(
        later_lines(&rows, &[0, 2]),
        vec![
            (vec![711, 712, 717, 802], None),
            (vec![711, 712, 802], None)
        ]
    );
    let mut draw = Draw(0x9e37_79b9_7f4a_7c15);
    for _ in 0..200 {
        let rows = (0..draw.below(120))
            .map(|_| row(1 + draw.below(60)))
            .collect::<Vec<_>>();
        let ends = (0..draw.below(8))
            .map(|_| draw.below(saturating_u32(rows.len()) + 1) as usize)
            .collect::<Vec<_>>();
        let swept = later_lines(&rows, &ends);
        for (end, got) in ends.iter().zip(swept) {
            assert_eq!(got, later_by_definition(&rows, *end), "end {end}");
        }
    }
}

/// One window per line, the first by `window_key`, as a page merges them.
fn deduped(mut windows: Vec<HitWindow>) -> Vec<HitWindow> {
    windows.sort_unstable_by_key(window_key);
    windows.dedup_by_key(|window| window.line);
    windows
}

/// A shown row owns the same lines among its page's rows plus their
/// neighbours as among every row of the file, whatever its own page made of
/// its window (a verified value may hold other lines than the stored one).
#[test]
fn neighbours_decide_owned_lines_as_every_row_of_the_file_would() {
    let mut draw = Draw(0x2545_f491_4f6c_dd1d);
    for _ in 0..2_000 {
        let rows = 1 + draw.below(40) as usize;
        let stored = (0..rows)
            .map(|_| {
                let line = 1 + draw.below(50);
                let matched_last = line + draw.below(2);
                let first = line.saturating_sub(draw.below(4)).max(1);
                HitWindow {
                    line,
                    matched_last,
                    first,
                    last: matched_last + draw.below(4),
                }
            })
            .collect::<Vec<_>>();
        let start = draw.below(saturating_u32(rows)) as usize;
        let end = start + draw.below(saturating_u32(rows - start) + 1) as usize;
        let shown = start..end;
        // What the page itself made of its rows' windows.
        let page = stored[shown.clone()]
            .iter()
            .map(|window| match draw.below(3) {
                0 => HitWindow {
                    first: window.line,
                    last: window.matched_last,
                    ..*window
                },
                _ => *window,
            })
            .collect::<Vec<_>>();
        let every = deduped(
            page.iter()
                .copied()
                .chain(
                    stored
                        .iter()
                        .enumerate()
                        .filter(|(row, _)| !shown.contains(row))
                        .map(|(_, window)| *window),
                )
                .collect(),
        );
        let near = deduped(
            page.iter()
                .copied()
                .chain(neighbours(&stored, &by_line(&stored), shown.clone()))
                .collect(),
        );
        for window in &page {
            assert_eq!(
                owned_lines(*window, &near),
                owned_lines(*window, &every),
                "{window:?} of {shown:?} in {stored:?}"
            );
        }
    }
}
