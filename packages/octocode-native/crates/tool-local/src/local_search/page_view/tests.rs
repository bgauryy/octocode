use super::*;
use crate::local_search::rows::owned_lines;

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
