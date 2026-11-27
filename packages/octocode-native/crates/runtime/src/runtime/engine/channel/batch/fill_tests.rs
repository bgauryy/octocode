//! The rows of one source-read call fill the page like water
//! ([`super::fill_page`]): a row that finishes under its share leaves the
//! rest to the rows still reading, each round finishes a row or ends the
//! fill, and a judged row is never read again.
use super::fill_page;
use octocode_runtime_base::execution::ExecutionError;
use octocode_runtime_base::response::outcome::DomainResult;
use octocode_runtime_base::response::read_share::{self, published_chars};
use octocode_runtime_base::security::CheckedQuery;
use serde_json::{Value, json};
use std::sync::Mutex;

/// A row of a source `need` chars long read with `share`: the whole source
/// when its row fits the share, else as much as fits beside its walk.
fn read(need: usize, share: usize) -> DomainResult {
    let whole = json!({"content": "x".repeat(need)});
    if published_chars(&whole).row <= share {
        return DomainResult::payload(whole, None);
    }
    let mut cut = json!({
        "content": "",
        "next": {"nextPage": {"kind": "continue", "tool": "localFetch", "input": {"queries": [{"path": "f"}]}}},
    });
    let mut text = share.min(need - 1);
    loop {
        cut["content"] = json!("x".repeat(text));
        let over = published_chars(&cut).row.saturating_sub(share);
        if over == 0 || text == 0 {
            return DomainResult::payload(cut, None);
        }
        text = text.saturating_sub(over);
    }
}

/// Every read the fill asked for: (row, share), in order.
struct Source {
    needs: Vec<usize>,
    reads: Mutex<Vec<(usize, usize)>>,
}

impl Source {
    fn new(needs: &[usize]) -> Self {
        Self {
            needs: needs.to_vec(),
            reads: Mutex::default(),
        }
    }

    fn queries(&self) -> Vec<CheckedQuery> {
        (0..self.needs.len())
            .map(|row| CheckedQuery::checked(json!({"path": format!("f{row}")})))
            .collect()
    }

    fn row(query: &Value) -> usize {
        query["path"].as_str().expect("path")[1..]
            .parse()
            .expect("row")
    }

    fn read(&self, queries: &[&CheckedQuery], share: usize) -> Vec<DomainResult> {
        queries
            .iter()
            .map(|query| {
                let row = Self::row(query);
                self.reads.lock().expect("reads").push((row, share));
                read(self.needs[row], share)
            })
            .collect()
    }

    /// The equal first read of every row, then the fill over the `room`.
    fn fill(&self, room: usize) -> Result<Vec<DomainResult>, ExecutionError> {
        let queries = self.queries();
        let share = room / queries.len();
        let first = self.read(&queries.iter().collect::<Vec<_>>(), share);
        fill_page(first, &queries, (room, share), |open, share| {
            Ok(self.read(open, share))
        })
    }

    fn reads_of(&self, row: usize) -> Vec<usize> {
        self.reads
            .lock()
            .expect("reads")
            .iter()
            .filter(|(read, _)| *read == row)
            .map(|(_, share)| *share)
            .collect()
    }
}

fn chars(rows: &[DomainResult]) -> usize {
    rows.iter().map(|row| published_chars(&row.data).row).sum()
}

/// The defect hoist X8 measured: a whole file batched with a short range was
/// cut at its equal share while the range left most of its own unused. Now
/// the file reads again with what the range left and fits whole.
#[test]
fn a_short_row_leaves_its_unused_share_to_the_row_still_reading() {
    let room = 20_000;
    let source = Source::new(&[18_000, 600]);
    let rows = source.fill(room).expect("fill");
    assert!(rows.iter().all(|row| !read_share::reads_on(&row.data)));
    assert!(chars(&rows) <= room);
    assert_eq!(
        source.reads_of(0),
        [room / 2, room - published_chars(&rows[1].data).row]
    );
    assert_eq!(
        source.reads_of(1),
        [room / 2],
        "a finished row is read once"
    );
}

/// Rows that all outgrow an even share have nothing to give each other:
/// each is read once.
#[test]
fn rows_that_all_read_on_are_each_read_once() {
    let room = 20_000;
    let source = Source::new(&[30_000, 25_000, 40_000]);
    let rows = source.fill(room).expect("fill");
    assert!(rows.iter().all(|row| read_share::reads_on(&row.data)));
    for row in 0..3 {
        assert_eq!(source.reads_of(row), [room / 3], "row {row}");
    }
    assert!(chars(&rows) <= room);
}

/// Each round finishes a row or ends the fill, so the page fills as far as
/// the rows' sizes allow: what the first finished rows leave finishes the
/// next one, and the last one reading takes all that is left. A row is read
/// at most once per row of the batch, each time with a wider share.
#[test]
fn each_round_finishes_a_row_until_the_page_is_full() {
    let room = 20_000;
    let source = Source::new(&[1_000, 4_000, 5_500, 30_000]);
    let rows = source.fill(room).expect("fill");
    let whole = [0, 1, 2]
        .iter()
        .map(|row| published_chars(&rows[*row].data).row)
        .sum::<usize>();
    assert!(rows[..3].iter().all(|row| !read_share::reads_on(&row.data)));
    assert!(read_share::reads_on(&rows[3].data));
    assert_eq!(*source.reads_of(3).last().expect("read"), room - whole);
    assert_eq!(
        source.reads_of(2).len(),
        2,
        "the third row finishes on what two left"
    );
    assert_eq!(source.reads_of(3).len(), 3);
    assert!(chars(&rows) <= room);
    for row in 0..4 {
        let reads = source.reads_of(row);
        assert!(reads.len() <= 4, "row {row}: {reads:?}");
        assert!(
            reads.windows(2).all(|pair| pair[0] < pair[1]),
            "row {row}: {reads:?}"
        );
    }
}

/// A failed read fails the fill as it fails the batch.
#[test]
fn a_failed_read_again_fails_the_fill() {
    let source = Source::new(&[18_000, 600]);
    let queries = source.queries();
    let first = source.read(&queries.iter().collect::<Vec<_>>(), 10_000);
    let failed = fill_page(first, &queries, (20_000, 10_000), |_, _| {
        Err(ExecutionError::WorkerFailed)
    });
    assert!(matches!(failed, Err(ExecutionError::WorkerFailed)));
}
